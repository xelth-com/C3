//! Local embeddings (M11, DESIGN §7 "Embeddings").
//!
//! The embedder is an OpenAI-compatible `/v1/embeddings` endpoint that MUST listen on this
//! machine: `http`/`https` with a loopback host (`127.0.0.1`, `localhost`, `[::1]`), no
//! userinfo, no query, no fragment, and the resolved address loopback too. Anything else is
//! refused — there is no cloud embedder and no key field. This module is compiled in every
//! build (including `--no-default-features`) so the loopback rule and the batch client can be
//! unit-tested without the store; only the vector storage and the HNSW leg
//! ([`crate::index::surreal`]) need the `index-surreal` feature.
//!
//! Nothing here follows a redirect (a 3xx would re-send project text elsewhere), stores no
//! mock or zero vector, and discards a whole batch whose response is the wrong shape,
//! dimension or count.

use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;

/// Resolve the embedder URL's host:port to loopback addresses NOW, refusing if the name resolves to
/// any non-loopback address (or to none). The returned addresses are pinned onto the request agent's
/// resolver so the connection reaches only what was validated here — a name that flips between
/// validation and the request (a rebound `localhost`) cannot reach another host. Never echoes the URL.
fn resolve_loopback_addrs(raw_url: &str) -> Result<Vec<SocketAddr>, String> {
    let u = url::Url::parse(raw_url).map_err(|_| CLOUD_EMBEDDER_MSG.to_string())?;
    let port = u.port_or_known_default().unwrap_or(80);
    let mut out = Vec::new();
    for a in host_addrs(&u, port)? {
        if !a.ip().is_loopback() {
            return Err(CLOUD_EMBEDDER_MSG.to_string());
        }
        out.push(a);
    }
    if out.is_empty() {
        return Err(CLOUD_EMBEDDER_MSG.to_string());
    }
    // IPv4 loopback first: `localhost` often resolves to `::1` ahead of `127.0.0.1`, while local
    // embedders (Ollama by default) listen on 127.0.0.1 only and an IPv6 loopback connect can hang
    // until the timeout on some hosts - trying 127.0.0.1 first never costs a reachable ::1 anything.
    out.sort_by_key(|a| a.is_ipv6());
    Ok(out)
}

/// The socket addresses of a URL's host: an IP literal is taken as parsed (the `url` crate hands
/// an IPv6 literal back bracketed, which a resolver may refuse — and a sandbox without IPv6 cannot
/// resolve `::1` at all); only a domain name goes to the resolver.
fn host_addrs(u: &url::Url, port: u16) -> Result<Vec<SocketAddr>, String> {
    match u.host() {
        Some(url::Host::Ipv4(ip)) => Ok(vec![SocketAddr::new(ip.into(), port)]),
        Some(url::Host::Ipv6(ip)) => Ok(vec![SocketAddr::new(ip.into(), port)]),
        Some(url::Host::Domain(d)) => Ok((d, port)
            .to_socket_addrs()
            .map_err(|_| CLOUD_EMBEDDER_MSG.to_string())?
            .collect()),
        None => Err(CLOUD_EMBEDDER_MSG.to_string()),
    }
}

/// The single refusal for a non-loopback / cloud embedder (never echoes the URL).
pub const CLOUD_EMBEDDER_MSG: &str =
    "a cloud embedder is not supported; the embedder must listen on this machine";

/// Validate an embedder URL against the loopback rule (DESIGN §7). `Ok(())` when it is an
/// `http`/`https` URL with a loopback host and no userinfo/query/fragment, whose host also
/// resolves only to loopback addresses; otherwise a one-line refusal that never echoes the URL
/// (it could be a pasted secret or an internal address).
pub fn validate_embedder_url(raw: &str) -> Result<(), String> {
    if raw.len() > 2048 {
        return Err("the embedder url is too long".to_string());
    }
    if raw.chars().any(|c| c.is_control()) {
        return Err("the embedder url must not contain control characters".to_string());
    }
    // The url crate's parse error can echo the input, so it is dropped.
    let u = url::Url::parse(raw).map_err(|_| "the embedder url is not a valid URL".to_string())?;
    if u.scheme() != "http" && u.scheme() != "https" {
        return Err(CLOUD_EMBEDDER_MSG.to_string());
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err("the embedder url must not contain a username or password".to_string());
    }
    if u.query().is_some() {
        return Err("the embedder url must not contain a query string".to_string());
    }
    if u.fragment().is_some() {
        return Err("the embedder url must not contain a fragment".to_string());
    }
    // The host must be a loopback literal or exactly "localhost" — checked on the PARSED host,
    // never a substring of the raw string, so `localhost.evil.example` is a plain domain and is
    // refused here before any resolution.
    match u.host() {
        Some(url::Host::Ipv4(ip)) if ip.is_loopback() => {}
        Some(url::Host::Ipv6(ip)) if ip.is_loopback() => {}
        Some(url::Host::Domain(d)) if d.eq_ignore_ascii_case("localhost") => {}
        _ => return Err(CLOUD_EMBEDDER_MSG.to_string()),
    }
    // The resolved address must be loopback too: a `localhost` a hosts file points elsewhere is
    // refused. An IP literal is taken as parsed (no resolver), so this is a no-op for
    // `127.0.0.1` / `[::1]`.
    let port = u.port_or_known_default().unwrap_or(80);
    let addrs = host_addrs(&u, port)?;
    let mut any = false;
    for a in addrs {
        any = true;
        if !a.ip().is_loopback() {
            return Err(CLOUD_EMBEDDER_MSG.to_string());
        }
    }
    if !any {
        return Err(CLOUD_EMBEDDER_MSG.to_string());
    }
    Ok(())
}

/// A validated local embedder: the loopback URL, the model name and the expected dimension.
/// No key is ever sent; there is no key field.
#[derive(Debug, Clone)]
pub struct Embedder {
    url: String,
    model: String,
    dimension: usize,
}

/// The result of an `index embed` run.
#[derive(Debug, Clone, Default)]
pub struct EmbedStats {
    /// Entities that received a fresh vector this run.
    pub embedded: usize,
    /// Entities considered (needed a vector) before `--limit`.
    pub considered: usize,
    /// Batches that failed whole (unavailable / wrong shape) and stored nothing.
    pub failed_batches: usize,
    /// The model name and dimension the vectors were stored with.
    pub model: String,
    pub dimension: usize,
}

impl Embedder {
    /// Build an embedder from an already-validated configuration ([`validate_embedder_url`] is
    /// applied when the configuration is loaded).
    pub fn new(url: impl Into<String>, model: impl Into<String>, dimension: usize) -> Self {
        Embedder {
            url: url.into(),
            model: model.into(),
            dimension,
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Embed one batch of texts, returning one vector per input in input order. The whole batch
    /// is discarded (an `Err`) when the endpoint is unavailable, answers with a redirect, or
    /// returns a payload whose count, dimension or finiteness is wrong — never a mock or zeros.
    /// `timeout` bounds the call (30 s for `index embed`, 2 s for a retrieval query).
    pub fn embed_batch(
        &self,
        inputs: &[String],
        timeout: Duration,
    ) -> Result<Vec<Vec<f32>>, String> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        // Resolve NOW and pin: the connection reaches only the loopback address(es) validated here,
        // never a name re-resolved at request time (a `localhost` that rebounds elsewhere is refused
        // before any connection). `ureq` would otherwise resolve the host a second time itself.
        let pinned = resolve_loopback_addrs(&self.url)?;
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(timeout)
            .timeout(timeout)
            // Never follow a redirect: a 3xx would re-send project text to another host.
            .redirects(0)
            .resolver(move |_netloc: &str| Ok(pinned.clone()))
            .build();
        let body = serde_json::json!({ "model": self.model, "input": inputs }).to_string();
        let resp = match agent
            .post(&self.url)
            .set("content-type", "application/json")
            .send_string(&body)
        {
            Ok(r) if (300..=399).contains(&r.status()) => {
                return Err("the embedder answered with a redirect (not followed)".to_string());
            }
            Ok(r) => r,
            Err(ureq::Error::Status(code, _)) => {
                return Err(format!("the embedder returned status {code}"));
            }
            Err(ureq::Error::Transport(t)) => {
                #[cfg(test)]
                eprintln!("embed transport error: {t:?}");
                #[cfg(not(test))]
                let _ = t;
                return Err("the embedder is unavailable".to_string());
            }
        };
        let text = resp
            .into_string()
            .map_err(|_| "the embedder response could not be read".to_string())?;
        self.parse_batch(&text, inputs.len())
    }

    /// Parse an OpenAI-compatible embeddings response into `count` vectors in `index` order,
    /// validating the count, each vector's dimension and finiteness. Any anomaly discards the
    /// whole batch.
    fn parse_batch(&self, text: &str, count: usize) -> Result<Vec<Vec<f32>>, String> {
        let v: serde_json::Value = serde_json::from_str(text)
            .map_err(|_| "the embedder response was not JSON".to_string())?;
        let data = v
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| "the embedder response had no data array".to_string())?;
        if data.len() != count {
            return Err(format!(
                "the embedder returned {} vectors for {count} inputs",
                data.len()
            ));
        }
        // Place each vector at its declared `index` (default: array order).
        let mut out: Vec<Option<Vec<f32>>> = vec![None; count];
        for (i, item) in data.iter().enumerate() {
            let idx = item
                .get("index")
                .and_then(|x| x.as_u64())
                .map(|x| x as usize)
                .unwrap_or(i);
            if idx >= count {
                return Err("the embedder returned an out-of-range index".to_string());
            }
            let emb = item
                .get("embedding")
                .and_then(|e| e.as_array())
                .ok_or_else(|| "an embedding was missing or not an array".to_string())?;
            if emb.len() != self.dimension {
                return Err(format!(
                    "an embedding had dimension {} (expected {})",
                    emb.len(),
                    self.dimension
                ));
            }
            let mut vec = Vec::with_capacity(emb.len());
            for n in emb {
                let f = n
                    .as_f64()
                    .ok_or_else(|| "an embedding value was not a number".to_string())?;
                if !f.is_finite() {
                    return Err("an embedding value was not finite".to_string());
                }
                vec.push(f as f32);
            }
            if out[idx].is_some() {
                return Err("the embedder returned a duplicate index".to_string());
            }
            out[idx] = Some(vec);
        }
        out.into_iter()
            .map(|o| o.ok_or_else(|| "the embedder omitted a vector".to_string()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_urls_accepted() {
        assert!(validate_embedder_url("http://127.0.0.1:11434/v1/embeddings").is_ok());
        assert!(validate_embedder_url("http://localhost:11434/v1/embeddings").is_ok());
        assert!(validate_embedder_url("http://[::1]:11434/v1/embeddings").is_ok());
        assert!(validate_embedder_url("https://127.0.0.1/v1/embeddings").is_ok());
    }

    #[test]
    fn non_loopback_urls_refused() {
        // A public host, a private LAN address, a look-alike domain, userinfo, and a bad scheme.
        for bad in [
            "http://8.8.8.8/v1/embeddings",
            "http://api.openai.com/v1/embeddings",
            "http://192.168.1.10/v1/embeddings",
            "http://10.0.0.5/v1/embeddings",
            "http://localhost.evil.example/v1/embeddings",
            "ftp://127.0.0.1/v1/embeddings",
        ] {
            assert!(validate_embedder_url(bad).is_err(), "{bad} must be refused");
        }
        // userinfo, query, fragment.
        assert!(validate_embedder_url("http://user:pass@127.0.0.1/v1/embeddings").is_err());
        assert!(validate_embedder_url("http://127.0.0.1/v1/embeddings?a=b").is_err());
        assert!(validate_embedder_url("http://127.0.0.1/v1/embeddings#frag").is_err());
    }

    #[test]
    fn refusals_never_echo_the_url() {
        let secret = "http://sk-secret-lan-host.internal.example/v1/embeddings";
        let e = validate_embedder_url(secret).unwrap_err();
        assert!(!e.contains("sk-secret"), "the url leaked: {e}");
        assert_eq!(e, CLOUD_EMBEDDER_MSG);
    }

    #[test]
    fn parse_batch_validates_count_dim_and_finite() {
        let e = Embedder::new("http://127.0.0.1/v1/embeddings", "m", 3);
        // Good response, out of order by index.
        let ok = r#"{"data":[{"index":1,"embedding":[4.0,5.0,6.0]},{"index":0,"embedding":[1.0,2.0,3.0]}]}"#;
        let v = e.parse_batch(ok, 2).unwrap();
        assert_eq!(v[0], vec![1.0, 2.0, 3.0]);
        assert_eq!(v[1], vec![4.0, 5.0, 6.0]);
        // Wrong dimension.
        assert!(e
            .parse_batch(r#"{"data":[{"index":0,"embedding":[1.0,2.0]}]}"#, 1)
            .is_err());
        // Count mismatch.
        assert!(e
            .parse_batch(r#"{"data":[{"index":0,"embedding":[1.0,2.0,3.0]}]}"#, 2)
            .is_err());
        // Non-finite.
        let nan = r#"{"data":[{"index":0,"embedding":[1.0,null,3.0]}]}"#;
        assert!(e.parse_batch(nan, 1).is_err());
    }

    #[test]
    fn embed_reaches_a_loopback_fake_via_localhost() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        // An in-process fake bound on EVERY loopback address `localhost` resolves to, at one port
        // (127.0.0.1 first, then ::1 at the same port, best effort), so the request reaches it
        // whichever address the pinned resolver tries first - the test must not depend on a
        // refused first address falling through to the second under load (was flaky).
        let first = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = first.local_addr().unwrap().port();
        let mut listeners = vec![first];
        if let Ok(l) = TcpListener::bind(("::1", port)) {
            listeners.push(l);
        }
        let n_listeners = listeners.len();
        let handles: Vec<_> = listeners
            .into_iter()
            .map(|listener| {
                std::thread::spawn(move || {
                    // Keep accepting until the test's final empty "stop" connection arrives (it
                    // sends nothing and closes), so a diagnostic probe or a client retry is
                    // answered too and the join at the end still returns.
                    while let Ok((mut stream, _)) = listener.accept() {
                        // Read the WHOLE request (headers + Content-Length body) before answering:
                        // answering and closing while the client is still sending the body makes
                        // Windows reset the connection and the client loses the response (the
                        // former flake of this test under a loaded suite).
                        let mut req = Vec::new();
                        let mut chunk = [0u8; 4096];
                        loop {
                            let n = match stream.read(&mut chunk) {
                                Ok(0) | Err(_) => break,
                                Ok(n) => n,
                            };
                            req.extend_from_slice(&chunk[..n]);
                            let text = String::from_utf8_lossy(&req);
                            if let Some(end) = text.find("\r\n\r\n") {
                                let len = text[..end]
                                    .lines()
                                    .find_map(|l| {
                                        let (k, v) = l.split_once(':')?;
                                        k.eq_ignore_ascii_case("content-length")
                                            .then(|| v.trim().parse::<usize>().ok())
                                            .flatten()
                                    })
                                    .unwrap_or(0);
                                if req.len() >= end + 4 + len {
                                    break;
                                }
                            }
                        }
                        if req.is_empty() {
                            break; // the stop connection
                        }
                        let body = r#"{"data":[{"index":0,"embedding":[1.0,2.0,3.0]}]}"#;
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(resp.as_bytes());
                        let _ = stream.flush();
                    }
                })
            })
            .collect();
        let e = Embedder::new(format!("http://localhost:{port}/v1/embeddings"), "m", 3);
        let diag = || {
            let pinned = resolve_loopback_addrs(&format!("http://localhost:{port}/v1/embeddings"));
            let probes: Vec<String> = ["127.0.0.1", "::1"]
                .iter()
                .map(|a| {
                    format!(
                        "{a}: {:?}",
                        std::net::TcpStream::connect_timeout(
                            &format!("{}:{port}", if *a == "::1" { "[::1]" } else { a })
                                .parse()
                                .unwrap(),
                            Duration::from_secs(1)
                        )
                        .map(|_| "ok")
                        .map_err(|e| e.to_string())
                    )
                })
                .collect();
            format!(
                "pinned={pinned:?} listeners={} probes={probes:?}",
                n_listeners
            )
        };
        let v = e
            .embed_batch(&["hi".to_string()], Duration::from_secs(5))
            .unwrap_or_else(|err| {
                panic!(
                    "the request reaches the loopback fake via localhost: {err}; {}",
                    diag()
                )
            });
        assert_eq!(v.len(), 1);
        assert_eq!(v[0], vec![1.0, 2.0, 3.0]);
        // A listener that never got a connection blocks in accept(); unblock it by connecting.
        for addr in ["127.0.0.1", "::1"] {
            let _ = std::net::TcpStream::connect((addr, port));
        }
        for h in handles {
            let _ = h.join();
        }
    }

    #[test]
    fn a_non_loopback_resolution_is_refused_before_connecting() {
        // A non-loopback address (an IP literal here; a rebound name resolves the same way) is a
        // resolver answer that must be refused before any connection.
        assert!(resolve_loopback_addrs("http://8.8.8.8:9/v1/embeddings").is_err());
        // localhost resolves to loopback and is pinned to those addresses.
        let addrs =
            resolve_loopback_addrs("http://localhost:11434/v1/embeddings").expect("loopback");
        assert!(!addrs.is_empty() && addrs.iter().all(|a| a.ip().is_loopback()));
        // embed_batch refuses the non-loopback host before sending anything.
        let e = Embedder::new("http://8.8.8.8:9/v1/embeddings", "m", 3);
        assert!(e
            .embed_batch(&["hi".to_string()], Duration::from_secs(1))
            .is_err());
    }
}
