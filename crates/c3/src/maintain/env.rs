//! The single injected seam between `c3 maintain` and the outside world: running a CLI and making
//! an HTTP GET. Production uses [`SystemEnv`]; the unit tests script a fake so no pass ever touches
//! a real CLI, the network or a model. Every external command of a pass goes through [`Env::run`],
//! every model-list HTTP read through [`Env::http_get`].

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::Path;
use std::time::{Duration, Instant};

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
        // the policy), and the deadline is absolute: each hop gets only what is left of it - its
        // name lookup and connect included ([`hop_agent`]) - ureq applies a request's timeout to
        // its body read as well, and [`fetch`] checks the deadline between the body's chunks.
        let ua = format!("c3/{}", env!("CARGO_PKG_VERSION"));
        let public_only = policy.public_only();
        fetch(url, policy, |current, remaining| {
            // (F14-1) the hop's name lookup first, under the deadline; the connect and the request
            // then get only what is left AFTER it
            let deadline = Instant::now() + remaining;
            let netloc = hop_netloc(current).ok_or_else(|| "bad url: no host".to_string())?;
            let (addrs, left) = resolve_hop(&netloc, deadline, public_only, system_lookup)
                .map_err(|e| c3_core::one_line(&e.to_string()))?;
            let agent = hop_agent(left, addrs);
            let mut req = agent.get(current).timeout(left).set("User-Agent", &ua);
            for (k, v) in headers {
                req = req.set(k, v);
            }
            match req.call() {
                Ok(resp) => {
                    // (F15-1) the address the connection was actually made to: a public source's
                    // answer from a local or private address is discarded before it is read
                    let peer = Some(resp.remote_addr().ip());
                    let status = resp.status();
                    if policy.redirects != Redirects::Never
                        && matches!(status, 301 | 302 | 303 | 307 | 308)
                    {
                        return Ok((
                            Hop::Redirect(resp.header("location").unwrap_or("").to_string()),
                            peer,
                        ));
                    }
                    Ok((Hop::Body(status, resp.into_reader()), peer))
                }
                Err(ureq::Error::Status(code, resp)) => {
                    Ok((Hop::Status(code), Some(resp.remote_addr().ip())))
                }
                Err(e) => Err(c3_core::one_line(&e.to_string())),
            }
        })
    }
}

/// (F14-1) The client of ONE hop of a GET, built AFTER the hop's name lookup ([`resolve_hop`]):
/// its connect phase gets only `connect_budget` - the time left once the lookup is done (ureq's
/// connect timeout takes precedence over the request's own, so it must not be the whole window) -
/// and it connects to the addresses that lookup vetted (`addrs`; no second lookup, so a name cannot
/// answer differently the second time). (F29-1) It never uses a proxy: C3 has no proxy setting
/// for `c3 maintain`, and ureq 2.12 would take one from `ALL_PROXY` / `HTTPS_PROXY` / `HTTP_PROXY`
/// if its `proxy-from-env` feature were on (off in C3's build, but a dependency could turn it on
/// through cargo's feature unification). Through a proxy the peer check of a public source would
/// see the proxy, never the origin, and the name lookup would vet the proxy's name - so the
/// environment's proxy is refused explicitly here. (The http engine honours the proxy on purpose,
/// for its own requests only.)
fn hop_agent(connect_budget: Duration, addrs: Vec<SocketAddr>) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(connect_budget)
        .redirects(0)
        .try_proxy_from_env(false)
        .resolver(move |_netloc: &str| Ok(addrs.clone()))
        .build()
}

/// The `host:port` a hop's URL connects to (the scheme's default port when none is written; an
/// IPv6 host keeps its brackets). `None` when the URL has no host.
fn hop_netloc(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    let host = u.host_str().filter(|h| !h.is_empty())?;
    Some(format!("{host}:{}", u.port_or_known_default()?))
}

/// The operating system's name lookup of a `host:port`.
fn system_lookup(netloc: String) -> std::io::Result<Vec<SocketAddr>> {
    use std::net::ToSocketAddrs as _;
    netloc.to_socket_addrs().map(|it| it.collect())
}

/// (F14-1) One hop's name lookup under the fetch's `deadline` ([`resolve_within`]; for a public
/// source a name with any non-global address is refused), then the time left for the connect and
/// the request - recomputed AFTER the lookup, so a lookup that took most of the window leaves them
/// only the rest, never a fresh budget. A lookup that ends at or past the deadline is a timeout.
fn resolve_hop<F>(
    netloc: &str,
    deadline: Instant,
    public_only: bool,
    lookup: F,
) -> std::io::Result<(Vec<SocketAddr>, Duration)>
where
    F: FnOnce(String) -> std::io::Result<Vec<SocketAddr>> + Send + 'static,
{
    let addrs = resolve_within(netloc, deadline, public_only, lookup)?;
    if addrs.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no address for {netloc}"),
        ));
    }
    let left = deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "the deadline passed during the name lookup",
            )
        })?;
    Ok((addrs, left))
}

/// (F14-1, F15-1) Resolve `netloc` (`host:port`) with `lookup` on its own thread, waiting at most
/// until `deadline` (a stalled lookup ends with a timeout instead of outliving the fetch's one
/// deadline; the thread is left to finish on its own). With `public_only`, a name of which ANY
/// address is local or private ([`is_public_address`]) is refused.
fn resolve_within<F>(
    netloc: &str,
    deadline: Instant,
    public_only: bool,
    lookup: F,
) -> std::io::Result<Vec<SocketAddr>>
where
    F: FnOnce(String) -> std::io::Result<Vec<SocketAddr>> + Send + 'static,
{
    use std::io::{Error, ErrorKind};
    let left = deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::TimedOut,
                "the deadline passed before the name lookup",
            )
        })?;
    let (tx, rx) = std::sync::mpsc::channel();
    let name = netloc.to_string();
    std::thread::Builder::new()
        .name("c3-maintain-dns".to_string())
        .spawn(move || {
            let _ = tx.send(lookup(name));
        })?;
    let addrs = match rx.recv_timeout(left) {
        Ok(r) => r?,
        Err(_) => {
            return Err(Error::new(
                ErrorKind::TimedOut,
                "the name lookup did not finish within the deadline",
            ))
        }
    };
    if public_only {
        if let Some(bad) = addrs.iter().find(|a| !is_public_address(a.ip())) {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                format!(
                    "the name resolves to a local or private address ({}); a public source is never fetched from it",
                    bad.ip()
                ),
            ));
        }
    }
    Ok(addrs)
}

/// (F15-1, F27-1) The IPv4 ranges that are not global unicast - a public source is never read from
/// an address in one of them: `(network, prefix length)`, IANA's special-purpose registry.
pub(crate) const NON_GLOBAL_V4: &[(Ipv4Addr, u8)] = &[
    (Ipv4Addr::new(0, 0, 0, 0), 8), // "this network" (a local route on Linux)
    (Ipv4Addr::new(10, 0, 0, 0), 8), // private
    (Ipv4Addr::new(100, 64, 0, 0), 10), // shared address space (CGNAT, overlays)
    (Ipv4Addr::new(127, 0, 0, 0), 8), // loopback
    (Ipv4Addr::new(169, 254, 0, 0), 16), // link-local
    (Ipv4Addr::new(172, 16, 0, 0), 12), // private
    (Ipv4Addr::new(192, 0, 0, 0), 24), // IETF protocol assignments
    (Ipv4Addr::new(192, 0, 2, 0), 24), // documentation (TEST-NET-1)
    (Ipv4Addr::new(192, 88, 99, 0), 24), // deprecated 6to4 relay anycast
    (Ipv4Addr::new(192, 168, 0, 0), 16), // private
    (Ipv4Addr::new(198, 18, 0, 0), 15), // benchmarking
    (Ipv4Addr::new(198, 51, 100, 0), 24), // documentation (TEST-NET-2)
    (Ipv4Addr::new(203, 0, 113, 0), 24), // documentation (TEST-NET-3)
    (Ipv4Addr::new(224, 0, 0, 0), 4), // multicast
    (Ipv4Addr::new(240, 0, 0, 0), 4), // reserved
    (Ipv4Addr::new(255, 255, 255, 255), 32), // limited broadcast
];

/// (F15-1, F27-1) The IPv6 ranges that are not global unicast: `(network, prefix length)`. An
/// IPv4-mapped address (`::ffff:0:0/96`) and one of the well-known NAT64 prefix (`64:ff9b::/96`)
/// are judged as the IPv4 address they carry ([`is_public_address`]).
pub(crate) const NON_GLOBAL_V6: &[(Ipv6Addr, u8)] = &[
    (Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0), 128), // unspecified
    (Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1), 128), // loopback
    (Ipv6Addr::new(0x64, 0xff9b, 1, 0, 0, 0, 0, 0), 48), // local-use IPv4/IPv6 translation
    (Ipv6Addr::new(0x100, 0, 0, 0, 0, 0, 0, 0), 64), // discard-only
    (Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0), 32), // documentation
    (Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 0), 7), // unique local
    (Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0), 10), // link-local
    (Ipv6Addr::new(0xfec0, 0, 0, 0, 0, 0, 0, 0), 10), // site-local (deprecated)
    (Ipv6Addr::new(0xff00, 0, 0, 0, 0, 0, 0, 0), 8), // multicast
];

/// Whether `a` lies in `net/len`.
fn in_v4(a: Ipv4Addr, net: Ipv4Addr, len: u8) -> bool {
    let mask = u32::MAX.checked_shl(32 - u32::from(len)).unwrap_or(0);
    u32::from(a) & mask == u32::from(net) & mask
}

/// Whether `a` lies in `net/len`.
fn in_v6(a: Ipv6Addr, net: Ipv6Addr, len: u8) -> bool {
    let mask = u128::MAX.checked_shl(128 - u32::from(len)).unwrap_or(0);
    u128::from(a) & mask == u128::from(net) & mask
}

/// (F15-1, F27-1) Whether `ip` is a global unicast address a catalog or page may be read from: in
/// none of the ranges of [`NON_GLOBAL_V4`] / [`NON_GLOBAL_V6`]. An IPv4-mapped IPv6 address
/// (`::ffff:a.b.c.d`) and a NAT64 one (`64:ff9b::a.b.c.d`) are judged as their IPv4 address.
/// Checked at the name lookup and at the peer of every hop.
pub(crate) fn is_public_address(ip: IpAddr) -> bool {
    let ip = match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None if in_v6(v6, Ipv6Addr::new(0x64, 0xff9b, 0, 0, 0, 0, 0, 0), 96) => {
                let [.., a, b, c, d] = v6.octets();
                IpAddr::V4(Ipv4Addr::new(a, b, c, d))
            }
            None => IpAddr::V6(v6),
        },
        v4 => v4,
    };
    match ip {
        IpAddr::V4(a) => !NON_GLOBAL_V4.iter().any(|(net, len)| in_v4(a, *net, *len)),
        IpAddr::V6(a) => !NON_GLOBAL_V6.iter().any(|(net, len)| in_v6(a, *net, *len)),
    }
}

/// (F15-1) Whether a public catalog or page URL names its host by a NAME: not an IP literal (any
/// form the URL parser reads as one, `https://2130706433/` included) and not `localhost` (any case,
/// trailing dots included). Checked at config validation and again at fetch.
pub fn is_named_public_host(url: &str) -> bool {
    match url::Url::parse(url).ok().as_ref().and_then(|u| u.host()) {
        Some(url::Host::Domain(d)) => {
            let d = d.trim_end_matches('.');
            !d.is_empty() && !d.eq_ignore_ascii_case("localhost")
        }
        _ => false,
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

impl GetPolicy {
    /// (F15-1) Whether this GET reads a public catalog or page (the [`Redirects::SameOrigin`]
    /// policy): its host must be a name ([`is_named_public_host`]), the name must resolve to public
    /// addresses only, and a response whose peer address is not public ([`is_public_address`]) is
    /// discarded before its body is read - at every hop.
    pub fn public_only(&self) -> bool {
        self.redirects == Redirects::SameOrigin
    }
}

/// One answer of a hop in [`fetch`]: a redirect to follow (its `location`), a body to read, or an
/// error status (no body).
pub(crate) enum Hop<R> {
    Redirect(String),
    Body(u16, R),
    Status(u16),
}

/// The redirect loop of a GET over an injected transport `hop(url, remaining)` (production: one
/// ureq request whose timeout is `remaining`) that answers a [`Hop`] and the peer address it came
/// from (`None`: unknown). The start URL and every redirect target are validated by the policy
/// ([`check_start`], [`redirect_target`]), at most [`MAX_REDIRECTS`] redirects are followed, the
/// deadline is absolute across all hops AND the body read (checked between its chunks), and the
/// body is read through [`read_capped`] with the policy's cap. (F15-1) Under a public policy every
/// answer whose peer is not a known public address is refused before anything of it is used.
pub(crate) fn fetch<R: std::io::Read>(
    url: &str,
    policy: GetPolicy,
    mut hop: impl FnMut(&str, Duration) -> Result<(Hop<R>, Option<IpAddr>), String>,
) -> Result<(u16, String), String> {
    check_start(url, policy)?;
    let deadline = Instant::now() + policy.timeout;
    let late = || {
        format!(
            "the request did not finish within {} ms (one deadline over every redirect and the body read)",
            policy.timeout.as_millis()
        )
    };
    let mut current = url.to_string();
    let mut hops = 0;
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(late)?;
        let (answer, peer) = match hop(&current, remaining) {
            Ok(a) => a,
            // (F14-1) a name lookup or connect cut off at the deadline is the deadline's error
            Err(e) if Instant::now() >= deadline => return Err(format!("{} ({e})", late())),
            Err(e) => return Err(e),
        };
        if policy.public_only() {
            check_peer(peer)?;
        }
        match answer {
            Hop::Redirect(location) => {
                hops += 1;
                if hops > MAX_REDIRECTS {
                    return Err("too many redirects".to_string());
                }
                current = redirect_target(url, &current, &location, policy.redirects)?;
                if policy.public_only() && !is_named_public_host(&current) {
                    return Err(
                        "refused a redirect to an IP address or localhost (a public source)"
                            .to_string(),
                    );
                }
            }
            Hop::Body(status, reader) => {
                // Read the body as bytes and lossily decode: the kimi `latest` marker answers with
                // no text content-type. A body that reaches the cap is an ERROR, never a truncated
                // prefix: the kimi installer is run after it is fetched, so a cut-off script must
                // never be written or executed.
                let buf = read_capped(reader, policy.cap, Some(deadline), &late)?;
                return Ok((status, String::from_utf8_lossy(&buf).into_owned()));
            }
            Hop::Status(code) => return Ok((code, String::new())),
        }
    }
}

/// (F15-1) The peer check of a public GET's answer: a known public address passes; a local or
/// private one, or an unknown one, is refused.
fn check_peer(peer: Option<IpAddr>) -> Result<(), String> {
    match peer {
        Some(ip) if is_public_address(ip) => Ok(()),
        Some(ip) => Err(format!(
            "refused: the answer came from a local or private address ({ip}); a public source is never read from one"
        )),
        None => Err(
            "refused: the answer's peer address is unknown (a public source is read only from a public address)"
                .to_string(),
        ),
    }
}

/// The start URL of a GET: https, with a host, without userinfo; (F15-1) for a public source a
/// host NAME, never an IP literal or `localhost`.
fn check_start(url: &str, policy: GetPolicy) -> Result<(), String> {
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
    if policy.public_only() && !is_named_public_host(url) {
        return Err("a public source is never fetched from an IP address or localhost".to_string());
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

/// The size of one read of a response body.
const BODY_CHUNK: usize = 8 * 1024;

/// Read a response body in chunks, refusing a body that reaches `cap` (the reader stops after
/// `cap + 1` bytes; a truncated download is never used). (F14-1) `deadline` is checked between the
/// chunks, so a body that trickles in ends with `late()` at the deadline instead of after it.
fn read_capped<R: std::io::Read>(
    mut r: R,
    cap: usize,
    deadline: Option<Instant>,
    late: &dyn Fn() -> String,
) -> Result<Vec<u8>, String> {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; BODY_CHUNK];
    loop {
        if deadline.is_some_and(|d| Instant::now() >= d) {
            return Err(late());
        }
        let want = (cap + 1 - buf.len()).min(BODY_CHUNK);
        let n = match r.read(&mut chunk[..want]) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                if deadline.is_some_and(|d| Instant::now() >= d) {
                    return Err(late());
                }
                return Err(c3_core::one_line(&e.to_string()));
            }
        };
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > cap {
            return Err(format!(
                "the response body exceeded the {cap}-byte cap (a truncated download is never used)"
            ));
        }
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
    use super::{
        check_peer, fetch, hop_agent, hop_netloc, is_named_public_host, is_public_address,
        is_secret_env, read_capped, redirect_target, resolve_hop, resolve_within, GetPolicy, Hop,
        Redirects, NON_GLOBAL_V4, NON_GLOBAL_V6,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    /// A public peer address for the fake transports (a public source's answer must come from one).
    const PUB: Option<IpAddr> = Some(IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)));

    fn no_late() -> String {
        "late".to_string()
    }

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
        assert_eq!(
            read_capped(under.as_slice(), 100, None, &no_late)
                .unwrap()
                .len(),
            90
        );
        let over = vec![b'x'; 120];
        assert!(read_capped(over.as_slice(), 100, None, &no_late).is_err());
        // exactly at the cap is accepted; one past it is not
        assert!(read_capped(vec![b'x'; 100].as_slice(), 100, None, &no_late).is_ok());
        assert!(read_capped(vec![b'x'; 101].as_slice(), 100, None, &no_late).is_err());
        // a body larger than one chunk is read whole
        let big = vec![b'y'; 3 * 8 * 1024 + 5];
        assert_eq!(
            read_capped(big.as_slice(), 1 << 20, None, &no_late)
                .unwrap()
                .len(),
            big.len()
        );
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
                Ok((Hop::Status(500), PUB))
            },
        );
        assert!(r.is_err() && asked.is_empty(), "{r:?} {asked:?}");
        assert!(fetch::<&[u8]>(
            "http://catalog.example/m",
            policy(5_000, Redirects::SameOrigin, 100),
            |_, _| Ok((Hop::Status(500), PUB))
        )
        .is_err());

        // a same-origin hop is followed, a userinfo hop is refused before it is requested
        let mut asked: Vec<String> = Vec::new();
        let r = fetch::<&[u8]>(
            "https://catalog.example/m",
            policy(5_000, Redirects::SameOrigin, 100),
            |u, _| {
                asked.push(u.to_string());
                Ok((
                    match asked.len() {
                        1 => Hop::Redirect("/m2".into()),
                        _ => Hop::Redirect("https://x:y@catalog.example/m3".into()),
                    },
                    PUB,
                ))
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
                Ok((Hop::Redirect(format!("/m{n}")), PUB))
            },
        );
        assert_eq!(r.unwrap_err(), "too many redirects");
        assert_eq!(n, 4, "the start and three redirects were requested");

        // the body: at the cap is read, one byte past it is an error (the reader stops at cap+1)
        let body = vec![b'x'; 100];
        let r = fetch(
            "https://catalog.example/m",
            policy(5_000, Redirects::SameOrigin, 100),
            |_, _| Ok((Hop::Body(200, body.as_slice()), PUB)),
        );
        assert_eq!(r.unwrap().1.len(), 100);
        let body = vec![b'x'; 101];
        assert!(fetch(
            "https://catalog.example/m",
            policy(5_000, Redirects::SameOrigin, 100),
            |_, _| Ok((Hop::Body(200, body.as_slice()), PUB)),
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
                Ok((Hop::Redirect("/again".into()), PUB))
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

    /// A body that never ends, one byte every `pause`.
    struct Trickle {
        pause: std::time::Duration,
    }
    impl std::io::Read for Trickle {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            std::thread::sleep(self.pause);
            buf[0] = b'x';
            Ok(1)
        }
    }

    /// A body that must never be read.
    struct Untouchable;
    impl std::io::Read for Untouchable {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            panic!("the body of a refused answer was read");
        }
    }

    /// (F14-1) A trickling body ends with the deadline's error at the deadline: the body is read in
    /// chunks and the one deadline is checked between them (the reviewer's 1-byte/s body).
    #[test]
    fn a_trickling_body_ends_at_the_deadline() {
        let start = std::time::Instant::now();
        let r = fetch(
            "https://catalog.example/m",
            policy(250, Redirects::SameOrigin, 1 << 20),
            |_, _| {
                Ok((
                    Hop::Body(
                        200,
                        Trickle {
                            pause: std::time::Duration::from_millis(20),
                        },
                    ),
                    PUB,
                ))
            },
        );
        let elapsed = start.elapsed();
        let err = r.unwrap_err();
        assert!(err.contains("did not finish within 250 ms"), "{err}");
        assert!(err.contains("the body read"), "{err}");
        assert!(
            elapsed < std::time::Duration::from_millis(1_000),
            "the body read ended at the deadline: {elapsed:?}"
        );
        // the same under an installer / endpoint policy (the deadline is not a public-only rule)
        let r = fetch(
            "https://code.kimi.ai/latest",
            policy(150, Redirects::Installer, 1 << 20),
            |_, _| {
                Ok((
                    Hop::Body(
                        200,
                        Trickle {
                            pause: std::time::Duration::from_millis(20),
                        },
                    ),
                    None,
                ))
            },
        );
        assert!(r.unwrap_err().contains("did not finish within 150 ms"));
    }

    /// (F14-1) The connect phase of a hop gets only the time that remains (ureq's connect timeout
    /// wins over the request's own, so the hop's client carries `remaining`, not the whole window),
    /// and a hop whose transport fails at the deadline reports the deadline's error.
    #[test]
    fn the_connect_phase_gets_only_the_remaining_time() {
        let addrs = vec!["93.184.216.34:443".parse::<SocketAddr>().unwrap()];
        let agent = format!(
            "{:?}",
            hop_agent(std::time::Duration::from_millis(1234), addrs)
        );
        assert!(agent.contains("timeout_connect: Some(1.234s)"), "{agent}");
        assert!(agent.contains("proxy: None"), "{agent}");
        let r = fetch::<&[u8]>(
            "https://catalog.example/m",
            policy(150, Redirects::SameOrigin, 100),
            |_, remaining| {
                // a stalled connect: the transport gives up exactly when its time is gone
                std::thread::sleep(remaining);
                Err("timed out connecting".to_string())
            },
        );
        let err = r.unwrap_err();
        assert!(
            err.contains("did not finish within 150 ms") && err.contains("timed out connecting"),
            "{err}"
        );
    }

    /// (F14-1) The name lookup runs under the same deadline: a stalled lookup ends with a timeout
    /// at the deadline. (F15-1) For a public source, a name of which any address is local or
    /// private is refused before a connection is made.
    #[test]
    fn the_name_lookup_is_bounded_and_refuses_a_private_address_for_a_public_source() {
        let start = std::time::Instant::now();
        let deadline = start + std::time::Duration::from_millis(150);
        let r = resolve_within("slow.example:443", deadline, false, |_| {
            std::thread::sleep(std::time::Duration::from_secs(3));
            Ok(Vec::new())
        });
        assert_eq!(r.unwrap_err().kind(), std::io::ErrorKind::TimedOut);
        assert!(start.elapsed() < std::time::Duration::from_millis(1_500));

        let mixed = || {
            Ok(vec![
                "93.184.216.34:443".parse::<SocketAddr>().unwrap(),
                "127.0.0.1:443".parse::<SocketAddr>().unwrap(),
            ])
        };
        let far = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let err = resolve_within("rebind.example:443", far, true, move |_| mixed()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(err.to_string().contains("127.0.0.1"), "{err}");
        // not a public source: the same answer is used as it is
        assert_eq!(
            resolve_within("rebind.example:443", far, false, move |_| mixed())
                .unwrap()
                .len(),
            2
        );
        // a public answer passes
        let ok = resolve_within("catalog.example:443", far, true, |_| {
            Ok(vec!["93.184.216.34:443".parse::<SocketAddr>().unwrap()])
        });
        assert_eq!(ok.unwrap().len(), 1);
    }

    /// (F15-1, F27-1) A public source is read only from a global unicast address: one row per
    /// non-global range (an address inside it, refused) plus the neighbours just outside the
    /// ranges (global, accepted). Every table entry's own network address is refused as well.
    #[test]
    fn the_address_classes_of_a_public_source() {
        for (a, range) in [
            ("0.0.0.0", "0.0.0.0/8"),
            ("0.0.0.1", "0.0.0.0/8"),
            ("0.255.255.255", "0.0.0.0/8"),
            ("10.1.2.3", "10/8"),
            ("100.64.0.1", "100.64/10 (CGNAT)"),
            ("100.127.255.254", "100.64/10 (CGNAT)"),
            ("127.0.0.1", "127/8"),
            ("127.8.9.10", "127/8"),
            ("169.254.169.254", "169.254/16"),
            ("172.16.0.1", "172.16/12"),
            ("172.31.255.255", "172.16/12"),
            ("192.0.0.8", "192.0.0.0/24"),
            ("192.0.2.1", "192.0.2.0/24"),
            ("192.88.99.1", "192.88.99.0/24"),
            ("192.168.1.1", "192.168/16"),
            ("198.18.0.1", "198.18/15"),
            ("198.19.255.255", "198.18/15"),
            ("198.51.100.7", "198.51.100.0/24"),
            ("203.0.113.9", "203.0.113.0/24"),
            ("224.0.0.1", "224/4"),
            ("239.255.255.250", "224/4"),
            ("240.0.0.1", "240/4"),
            ("255.255.255.255", "broadcast"),
            ("::", "::/128"),
            ("::1", "::1"),
            ("::ffff:127.0.0.1", "IPv4-mapped, judged as its IPv4"),
            ("::ffff:10.0.0.1", "IPv4-mapped, judged as its IPv4"),
            ("::ffff:100.64.0.1", "IPv4-mapped, judged as its IPv4"),
            ("64:ff9b::a00:1", "NAT64, judged as its IPv4 (10.0.0.1)"),
            ("64:ff9b:1::1", "64:ff9b:1::/48"),
            ("100::1", "100::/64"),
            ("2001:db8::1", "2001:db8::/32"),
            ("fc00::1", "fc00::/7"),
            ("fd12:3456::1", "fc00::/7"),
            ("fe80::1", "fe80::/10"),
            ("fec0::1", "fec0::/10"),
            ("feff::1", "fec0::/10"),
            ("ff02::1", "ff00::/8"),
        ] {
            assert!(
                !is_public_address(a.parse().unwrap()),
                "{a} ({range}) is not public"
            );
        }
        for (net, _) in NON_GLOBAL_V4 {
            assert!(!is_public_address(IpAddr::V4(*net)), "{net}");
        }
        for (net, _) in NON_GLOBAL_V6 {
            assert!(!is_public_address(IpAddr::V6(*net)), "{net}");
        }
        for a in [
            "93.184.216.34",
            "8.8.8.8",
            "1.0.0.1",
            "100.63.255.255",
            "100.128.0.1",
            "172.15.255.255",
            "172.32.0.1",
            "192.0.1.1",
            "192.0.3.1",
            "192.88.98.1",
            "198.17.255.255",
            "198.20.0.1",
            "198.51.101.1",
            "203.0.114.1",
            "223.255.255.254",
            "2606:4700::1111",
            "2001:4860:4860::8888",
            "::ffff:93.184.216.34",
            "64:ff9b::808:808",
            "100:0:0:1::1",
            "2001:db9::1",
        ] {
            assert!(is_public_address(a.parse().unwrap()), "{a} is public");
        }
        // the lookup check refuses such a name (here a CGNAT and a benchmarking address)
        let far = std::time::Instant::now() + std::time::Duration::from_secs(5);
        for bad in ["100.64.0.1:443", "198.18.0.1:443", "[fec0::1]:443"] {
            let err = resolve_within("catalog.example:443", far, true, move |_| {
                Ok(vec![bad.parse::<SocketAddr>().unwrap()])
            })
            .unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied, "{bad}");
        }
        // and the peer check of every hop refuses such an answer
        assert!(check_peer(Some("100.64.0.1".parse().unwrap())).is_err());
        assert!(check_peer(Some("0.0.0.1".parse().unwrap())).is_err());
        assert!(check_peer(Some("93.184.216.34".parse().unwrap())).is_ok());
    }

    /// (F14-1) The time left for a hop's connect and request is recomputed AFTER its name lookup: a
    /// lookup that takes most of the deadline leaves the connect only the rest (never the budget
    /// computed before the lookup), and a lookup that ends past the deadline is a timeout before
    /// any connect.
    #[test]
    fn the_connect_gets_the_time_left_after_the_name_lookup() {
        let start = std::time::Instant::now();
        let deadline = start + std::time::Duration::from_millis(1_000);
        let (addrs, left) = resolve_hop("catalog.example:443", deadline, true, |_| {
            std::thread::sleep(std::time::Duration::from_millis(800));
            Ok(vec!["93.184.216.34:443".parse::<SocketAddr>().unwrap()])
        })
        .unwrap();
        assert_eq!(addrs.len(), 1);
        assert!(
            left <= std::time::Duration::from_millis(200),
            "the connect budget is what the lookup left: {left:?}"
        );
        assert!(!left.is_zero());
        // the hop's client carries exactly that budget
        let agent = format!("{:?}", hop_agent(left, addrs));
        let budget = agent
            .split("timeout_connect: Some(")
            .nth(1)
            .and_then(|s| s.split(')').next())
            .unwrap_or("?")
            .to_string();
        assert!(
            budget.ends_with("ms") || budget.ends_with("µs"),
            "a sub-second connect budget, not the hop's whole second: {budget} ({agent})"
        );
        // a lookup that outlasts the deadline never reaches a connect
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(150);
        let err = resolve_hop("slow.example:443", deadline, true, |_| {
            std::thread::sleep(std::time::Duration::from_millis(600));
            Ok(vec!["93.184.216.34:443".parse::<SocketAddr>().unwrap()])
        })
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        // the netloc of a hop: the scheme's default port, an IPv6 host in brackets
        assert_eq!(
            hop_netloc("https://openrouter.ai/api/v1/models").as_deref(),
            Some("openrouter.ai:443")
        );
        assert_eq!(
            hop_netloc("https://[::1]:8443/m").as_deref(),
            Some("[::1]:8443")
        );
    }

    /// (F15-1) A public source's URL names its host: an IP literal (any form) or `localhost` (any
    /// case, trailing dot) is refused before anything is requested; an installer or endpoint request
    /// is not judged by this rule.
    #[test]
    fn a_public_source_on_an_ip_literal_or_localhost_is_never_requested() {
        for url in [
            "https://127.0.0.1/m",
            "https://10.0.0.7/m",
            "https://[::1]/m",
            "https://[fe80::1]/m",
            "https://2130706433/m",
            "https://0x7f.1/m",
            "https://localhost/m",
            "https://LOCALHOST./m",
            "https://localhost:8443/m",
            "https://93.184.216.34/m",
        ] {
            assert!(!is_named_public_host(url), "{url}");
            let mut asked = 0;
            let r = fetch::<&[u8]>(url, policy(5_000, Redirects::SameOrigin, 100), |_, _| {
                asked += 1;
                Ok((Hop::Status(200), PUB))
            });
            assert!(r.is_err() && asked == 0, "{url}: {r:?}");
        }
        for url in [
            "https://openrouter.ai/api/v1/models",
            "https://catalog.example./m",
            "https://localhost.example/m",
        ] {
            assert!(is_named_public_host(url), "{url}");
        }
        // the installer policy keeps its own rules
        assert!(fetch::<&[u8]>(
            "https://127.0.0.1/latest",
            policy(5_000, Redirects::Installer, 100),
            |_, _| Ok((Hop::Status(404), None))
        )
        .is_ok());
    }

    /// (F15-1) After the connection is made, a public source's answer whose peer is local or
    /// private - or unknown - is discarded before its body is read, at every hop (the DNS-rebinding
    /// case: a public name that answers from 127.0.0.1); the next hop is never requested.
    #[test]
    fn a_public_answer_from_a_private_peer_is_discarded_before_its_body_is_read() {
        let lo = Some(IpAddr::V4(Ipv4Addr::LOCALHOST));
        let r = fetch(
            "https://catalog.example/m",
            policy(5_000, Redirects::SameOrigin, 100),
            |_, _| Ok((Hop::Body(200, Untouchable), lo)),
        );
        let err = r.unwrap_err();
        assert!(
            err.contains("local or private address (127.0.0.1)"),
            "{err}"
        );
        // a redirect from a private peer is not followed
        let mut asked: Vec<String> = Vec::new();
        let r = fetch(
            "https://catalog.example/m",
            policy(5_000, Redirects::SameOrigin, 100),
            |u, _| {
                asked.push(u.to_string());
                Ok(match asked.len() {
                    1 => (Hop::Redirect("/m2".into()), PUB),
                    _ => (
                        Hop::Body(200, Untouchable),
                        Some("10.0.0.5".parse().unwrap()),
                    ),
                })
            },
        );
        assert!(r.unwrap_err().contains("(10.0.0.5)"));
        assert_eq!(asked.len(), 2, "the second hop is checked too");
        let mut asked = 0;
        let r = fetch::<Untouchable>(
            "https://catalog.example/m",
            policy(5_000, Redirects::SameOrigin, 100),
            |_, _| {
                asked += 1;
                Ok((Hop::Redirect("/m2".into()), lo))
            },
        );
        assert!(r.is_err() && asked == 1, "{r:?}");
        // an unknown peer is refused for a public source; an installer GET does not need one
        assert!(fetch(
            "https://catalog.example/m",
            policy(5_000, Redirects::SameOrigin, 100),
            |_, _| Ok((Hop::Body(200, Untouchable), None))
        )
        .unwrap_err()
        .contains("unknown"));
        let body = b"ok".to_vec();
        assert_eq!(
            fetch(
                "https://code.kimi.ai/latest",
                policy(5_000, Redirects::Installer, 100),
                |_, _| Ok((Hop::Body(200, body.as_slice()), lo))
            )
            .unwrap()
            .1,
            "ok"
        );
    }
}
