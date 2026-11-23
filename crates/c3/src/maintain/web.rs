//! The public model sources of the model step: catalogs (a JSON list of models, by default
//! OpenRouter's `GET https://openrouter.ai/api/v1/models`) and plain pages (a vendor's documentation
//! page, read with a configured regex). Both are fetched with a plain KEYLESS GET through
//! [`Env::http_get`] - no credential is ever attached, redirects stay on https and on the start's
//! own host and port ([`PUBLIC_GET`]), one deadline covers every hop, the body is capped while it
//! is read - and both are UNTRUSTED input: they only ever yield
//! candidate id strings, each of which must pass [`valid_model_id`]. A catalog id never seats
//! anything by itself; the smoke on the entry's own plan decides.
//!
//! The step follows `C3_MAINTAIN_MODELS` (`off` = no fetch) and its own switch, default ON:
//! `C3_MODEL_WEB=off` or `"model_web": false` in the C3 config. `"model_catalogs"` replaces the
//! default catalog list, `"model_pages"` adds pages.

use std::time::Duration;

use super::env::{Env, GetPolicy, Redirects};
use super::models::{ListPrice, SourceList};
use super::version::valid_model_id;

/// The catalogs fetched when the config names none.
pub const DEFAULT_CATALOGS: &[&str] = &["https://openrouter.ai/api/v1/models"];

/// The largest catalog or page body accepted (bytes); a larger one is dropped whole.
pub const BODY_CAP: usize = 4 * 1024 * 1024;

/// The most ids one catalog or page may yield; the rest are ignored.
pub const MAX_IDS: usize = 5000;

/// The timeout of one public GET.
const TIMEOUT: Duration = Duration::from_secs(15);

/// A configured documentation page: a label, an https URL and a regex with ONE capture group whose
/// every capture is a candidate id.
#[derive(Debug, Clone)]
pub struct Page {
    pub name: String,
    pub url: String,
    pub pattern: regex::Regex,
}

/// Whether the public sources are fetched: `C3_MODEL_WEB=off` (environment wins) or `"model_web":
/// false` in the C3 config turns them off; default ON.
pub fn enabled() -> bool {
    if let Ok(v) = std::env::var("C3_MODEL_WEB") {
        return !v.trim().eq_ignore_ascii_case("off");
    }
    match crate::c3config::load() {
        Ok(Some(cfg)) => cfg.model_web,
        _ => true,
    }
}

/// The catalogs and pages to fetch, from the C3 config (defaults when there is none or it does not
/// load).
pub fn configured_sources() -> (Vec<String>, Vec<Page>) {
    match crate::c3config::load() {
        Ok(Some(cfg)) => {
            let catalogs = match &cfg.model_catalogs {
                Some(list) => list.iter().map(|c| c.url.clone()).collect(),
                None => DEFAULT_CATALOGS.iter().map(|s| s.to_string()).collect(),
            };
            let pages = cfg
                .model_pages
                .iter()
                .filter_map(|p| {
                    regex::Regex::new(&p.pattern).ok().map(|re| Page {
                        name: p.name.clone(),
                        url: p.url.clone(),
                        pattern: re,
                    })
                })
                .collect();
            (catalogs, pages)
        }
        _ => (
            DEFAULT_CATALOGS.iter().map(|s| s.to_string()).collect(),
            Vec::new(),
        ),
    }
}

/// (F16-1, F17-5) The source name of a catalog: `web:<url>` - its FULL canonical URL, the identity
/// of the source: the lower-case scheme and host, the port (the scheme's default when none is
/// written), the path and the query (`web:https://openrouter.ai:443/api/v1/models`). Two catalogs
/// on one host are two sources, each fetched and recorded under its own name, each trusted only
/// by its own config entry.
pub fn catalog_source(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    if u.scheme() != "https" {
        return None;
    }
    let host = u.host_str()?.to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let query = u.query().map(|q| format!("?{q}")).unwrap_or_default();
    Some(format!(
        "web:https://{host}:{}{}{query}",
        u.port_or_known_default().unwrap_or(443),
        u.path()
    ))
}

/// A source name as it is SHOWN - in a printed line, a report, the hook line, anything a person or
/// an agent reads. A catalog's name ([`catalog_source`]) is the full URL, and its query may hold a
/// token (an operator may write `?key=...` although the GET is keyless), so it is shown without
/// the query: `web:` + scheme, host, the port only when it is not 443, and the path, with
/// ` (query not shown)` appended when a query was dropped. Every other source name (a legacy
/// `web:<host>`, `page:<name>`, an engine list) is shown as it is. The stored key - `state.json`,
/// trust, comparison - stays the full URL.
pub fn catalog_source_display(source: &str) -> String {
    let Some(rest) = source.strip_prefix("web:").filter(|r| r.contains("://")) else {
        return source.to_string();
    };
    const NOTE: &str = " (query not shown)";
    match url::Url::parse(rest) {
        Ok(u) => {
            let port = match u.port_or_known_default() {
                Some(443) | None => String::new(),
                Some(p) => format!(":{p}"),
            };
            let dropped = u.query().is_some_and(|q| !q.is_empty());
            format!(
                "web:{}://{}{port}{}{}",
                u.scheme(),
                u.host_str().unwrap_or(""),
                u.path(),
                if dropped { NOTE } else { "" }
            )
        }
        // not a URL (never written by this build): cut at the query by text
        Err(_) => {
            let shown = rest.split(['?', '#']).next().unwrap_or("");
            let dropped = rest.contains('?');
            format!("web:{shown}{}", if dropped { NOTE } else { "" })
        }
    }
}

/// (F16-1, F17-5) Whether a source name is a catalog record of an older build, keyed by its host
/// alone (`web:<host>`): read without error, but never trusted for telemetry - which of the host's
/// URLs its ids came from is unknown - and replaced once a catalog of that host is fetched again.
pub fn is_legacy_catalog_source(source: &str) -> bool {
    source
        .strip_prefix("web:")
        .is_some_and(|rest| !rest.contains("://"))
}

/// The host of a catalog source name, of either form (`web:<url>` or the legacy `web:<host>`).
pub fn catalog_source_host(source: &str) -> Option<String> {
    let rest = source.strip_prefix("web:")?;
    if !rest.contains("://") {
        return Some(rest.to_ascii_lowercase());
    }
    url::Url::parse(rest)
        .ok()?
        .host_str()
        .map(|h| h.to_ascii_lowercase())
}

/// Whether a source name is a public one (a catalog or a page) - fetched keyless, its ids feed
/// successor candidates. Telemetry trusts only some of them ([`trusted_sources`]).
pub fn is_public_source(source: &str) -> bool {
    source.starts_with("web:") || source.starts_with("page:")
}

/// (F02-19) A public source whose ids telemetry may send by name, and the vendor class it
/// attributes an id WITHOUT a `<vendor>/` part to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedSource {
    /// The source name (`web:<url>` / `page:<name>`).
    pub source: String,
    /// The vendor class of the source's own host (a vendor's documentation page on `docs.z.ai` is
    /// `zai`) - the attribution of its plain ids; `None` (any other host, and OpenRouter, whose ids
    /// carry their vendor) attributes them to nobody.
    pub class: Option<&'static str>,
}

/// (F02-19) The public sources whose ids are PUBLISHED names telemetry may send: the catalog
/// URLs shipped in the code ([`DEFAULT_CATALOGS`] - OpenRouter), always; a configured catalog or
/// page only when its config entry says `"public": true` explicitly. Separate from what is fetched:
/// a configured source feeds successor candidates either way, but it may be an internal one, and a
/// name it lists is no proof that the name is no private endpoint's secret. (F16-1, F17-5) Trust
/// is per catalog URL ([`catalog_source`]), never per host: another URL on a trusted host - the
/// built-in catalog's included - is trusted only by its own entry, and a legacy host-keyed record
/// ([`is_legacy_catalog_source`]) matches no trusted source.
pub fn trusted_sources(cfg: Option<&crate::c3config::C3Config>) -> Vec<TrustedSource> {
    let mut out: Vec<TrustedSource> = Vec::new();
    let mut add = |source: String, url: &str| {
        if out.iter().any(|t| t.source == source) {
            return;
        }
        let class = crate::telemetry::classes::vendor_by_host(url)
            .map(|v| v.class)
            .filter(|c| *c != crate::telemetry::classes::OPENROUTER.class);
        out.push(TrustedSource { source, class });
    };
    for url in DEFAULT_CATALOGS {
        if let Some(source) = catalog_source(url) {
            add(source, url);
        }
    }
    if let Some(cfg) = cfg {
        for c in cfg.model_catalogs.iter().flatten().filter(|c| c.public) {
            if let Some(source) = catalog_source(&c.url) {
                add(source, &c.url);
            }
        }
        for p in cfg.model_pages.iter().filter(|p| p.public) {
            add(format!("page:{}", p.name), &p.url);
        }
    }
    out
}

/// The model part of a catalog id (`qwen/qwen4-max` -> `qwen4-max`); a plain id is itself.
pub fn strip_vendor(id: &str) -> &str {
    id.split_once('/').map(|(_, m)| m).unwrap_or(id)
}

/// Collect every configured public source: (F16-1, F17-5) every distinct catalog URL under its own
/// name ([`catalog_source`]), every page under its label. A source that fails (network, status,
/// cap, parse) is left out; a source with no valid id is left out.
pub fn collect(env: &dyn Env, catalogs: &[String], pages: &[Page]) -> Vec<SourceList> {
    let mut out: Vec<SourceList> = Vec::new();
    let mut fetched: Vec<String> = Vec::new();
    for url in catalogs {
        let Some(source) = catalog_source(url) else {
            continue;
        };
        if fetched.contains(&source) {
            continue;
        }
        fetched.push(source.clone());
        let Some(body) = keyless_get(env, url) else {
            continue;
        };
        let list = parse_catalog(&body);
        if !list.ids.is_empty() {
            out.push(SourceList { source, ..list });
        }
    }
    for p in pages {
        let source = format!("page:{}", p.name);
        if out.iter().any(|s| s.source == source) || !p.url.starts_with("https://") {
            continue;
        }
        let Some(body) = keyless_get(env, &p.url) else {
            continue;
        };
        let ids = parse_page(&body, &p.pattern);
        if !ids.is_empty() {
            out.push(SourceList {
                source,
                ids,
                ..Default::default()
            });
        }
    }
    out
}

/// A keyless GET of a public source: https only, (F15-1) on a host NAME - an IP literal or
/// `localhost` is never requested, in test mode either - no header but the client's own generic
/// User-Agent (`c3/<version>`), under [`PUBLIC_GET`] (which also discards an answer from a local or
/// private address). `None` on any failure.
///
/// Under `CODEX_CONSULT_TEST_MODE` no request of the real [`SystemEnv`](super::env::SystemEnv)
/// reaches the network: `C3_MAINTAIN_TEST_HTTP`
/// (`<url>|<file>;...`) maps a URL to a local file, an unmapped URL fails, and every requested URL
/// is appended to `C3_MAINTAIN_TEST_HTTP_LOG` when set - so the integration tests can both feed a
/// catalog and prove that a switched-off pass fetched nothing.
fn keyless_get(env: &dyn Env, url: &str) -> Option<String> {
    if !url.starts_with("https://") || !super::env::is_named_public_host(url) {
        return None;
    }
    if env.is_system() && c3_core::test_hooks::mode_on() {
        if let Ok(log) = std::env::var("C3_MAINTAIN_TEST_HTTP_LOG") {
            if !log.trim().is_empty() {
                use std::io::Write as _;
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(log.trim())
                {
                    let _ = writeln!(f, "{url}");
                }
            }
        }
        let map = std::env::var("C3_MAINTAIN_TEST_HTTP").unwrap_or_default();
        let file = map.split(';').find_map(|pair| {
            let (u, f) = pair.split_once('|')?;
            (u.trim() == url).then(|| f.trim().to_string())
        });
        // the real environment under test mode never reaches the network: an unmapped URL fails
        return file.and_then(|f| {
            std::fs::read_to_string(f)
                .ok()
                .filter(|b| b.len() <= BODY_CAP)
        });
    }
    get_via_env(env, url)
}

/// The policy of a public GET (F02-12, F02-13): redirects only to the start's own host and port
/// with no userinfo, re-validated at every hop (never the kimi installer's mirror pair); ONE 15 s
/// deadline over every hop, its name lookup, connect and body read (F14-1); the reader stops at
/// [`BODY_CAP`] + 1 bytes; (F15-1) never an IP literal or `localhost`, never an answer from a local
/// or private address ([`GetPolicy::public_only`]).
pub const PUBLIC_GET: GetPolicy = GetPolicy {
    timeout: TIMEOUT,
    redirects: Redirects::SameOrigin,
    cap: BODY_CAP,
};

fn get_via_env(env: &dyn Env, url: &str) -> Option<String> {
    match env.http_get(url, &[], PUBLIC_GET) {
        Ok((status, body)) if (200..300).contains(&status) && body.len() <= BODY_CAP => Some(body),
        _ => None,
    }
}

/// Parse a catalog body (`{"data":[{"id":"<vendor>/<model>","created":<unix secs>,"pricing":
/// {"prompt":"<usd>","completion":"<usd>"}}, ...]}`) into its ids, their `created` stamps and
/// (0.2.5 D3) their list prices ([`list_price`]) - a [`SourceList`] whose `source` the caller sets.
/// An id with a `:variant` suffix (`:free`, `:batch`) is skipped; the vendor part must be a
/// lower-case slug and the model part must pass [`valid_model_id`] (with no further `/`); a plain id
/// without a vendor is kept when it passes. At most [`MAX_IDS`] ids, de-duplicated, in catalog
/// order.
pub fn parse_catalog(body: &str) -> SourceList {
    let mut out = SourceList::default();
    if body.len() > BODY_CAP {
        return out;
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return out;
    };
    let Some(arr) = v.get("data").and_then(|d| d.as_array()) else {
        return out;
    };
    let (ids, created, prices) = (&mut out.ids, &mut out.created, &mut out.prices);
    let mut seen = std::collections::HashSet::new();
    for m in arr {
        if ids.len() >= MAX_IDS {
            break;
        }
        let Some(id) = m.get("id").and_then(|x| x.as_str()) else {
            continue;
        };
        let Some(id) = catalog_id(id) else {
            continue;
        };
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(c) = m.get("created").and_then(|x| x.as_i64()) {
            created.insert(id.clone(), c);
        }
        if let Some(p) = list_price(m) {
            prices.insert(id.clone(), p);
        }
        ids.push(id);
    }
    out
}

/// (0.2.5 D3) The list price of one catalog entry: `pricing.prompt` and `pricing.completion`, each
/// a decimal string (OpenRouter's form) or a number, kept only when it is a positive finite number
/// (`"0"` - a free variant - and OpenRouter's `"-1"` - a router, no fixed price - are no price).
/// `None` when neither field is.
fn list_price(m: &serde_json::Value) -> Option<ListPrice> {
    let pricing = m.get("pricing")?;
    let field = |k: &str| -> Option<f64> {
        let n = match pricing.get(k)? {
            serde_json::Value::String(s) => s.trim().parse::<f64>().ok()?,
            serde_json::Value::Number(n) => n.as_f64()?,
            _ => return None,
        };
        (n.is_finite() && n > 0.0).then_some(n)
    };
    let p = ListPrice {
        prompt: field("prompt"),
        completion: field("completion"),
    };
    (p.prompt.is_some() || p.completion.is_some()).then_some(p)
}

/// One catalog id through the shape rules, or `None`.
fn catalog_id(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.contains(':') || !valid_model_id(raw) {
        return None; // a `:variant` (free / batch / beta ...) is not a model of its own
    }
    match raw.split_once('/') {
        Some((vendor, model)) => {
            let vendor_ok = !vendor.is_empty()
                && vendor.len() <= 40
                && vendor.bytes().all(|b| {
                    b.is_ascii_lowercase()
                        || b.is_ascii_digit()
                        || b == b'-'
                        || b == b'_'
                        || b == b'.'
                });
            if !vendor_ok || model.contains('/') || !valid_model_id(model) {
                return None;
            }
            Some(raw.to_string())
        }
        None => Some(raw.to_string()),
    }
}

/// The ids of a page: every capture of group 1 of `pattern` that passes [`valid_model_id`],
/// de-duplicated, at most [`MAX_IDS`].
pub fn parse_page(body: &str, pattern: &regex::Regex) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if body.len() > BODY_CAP {
        return out;
    }
    for c in pattern.captures_iter(body) {
        if out.len() >= MAX_IDS {
            break;
        }
        let Some(m) = c.get(1) else {
            continue;
        };
        let id = m.as_str().trim();
        if valid_model_id(id) && !out.iter().any(|x| x == id) {
            out.push(id.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maintain::env::RunResult;
    use std::path::Path;

    struct FakeHttp {
        body: String,
        status: u16,
        seen: std::cell::RefCell<Vec<(String, usize, GetPolicy)>>,
    }
    impl Env for FakeHttp {
        fn run(&self, _p: &str, _a: &[&str], _e: &[(&str, &str)], _c: Option<&Path>) -> RunResult {
            RunResult::default()
        }
        fn http_get(
            &self,
            url: &str,
            headers: &[(&str, &str)],
            policy: GetPolicy,
        ) -> Result<(u16, String), String> {
            self.seen
                .borrow_mut()
                .push((url.to_string(), headers.len(), policy));
            Ok((self.status, self.body.clone()))
        }
    }

    #[test]
    fn the_catalog_parser_splits_vendor_skips_variants_and_drops_invalid_ids() {
        let body = r#"{"data":[
            {"id":"qwen/qwen4-max","created":1794795526},
            {"id":"qwen/qwen4-max:free","created":1794795526},
            {"id":"openai/gpt-6.1-sol:batch"},
            {"id":"google/gemini-4-hydrogen","created":1794000000},
            {"id":"bad vendor/x-1"},
            {"id":"Vendor/x-1"},
            {"id":"acme/a\"b"},
            {"id":"acme/one/two"},
            {"id":"plain-model-2"},
            {"id":"qwen/qwen4-max"},
            {"name":"no id"}
        ]}"#;
        let SourceList { ids, created, .. } = parse_catalog(body);
        assert_eq!(
            ids,
            vec![
                "qwen/qwen4-max",
                "google/gemini-4-hydrogen",
                "plain-model-2"
            ]
        );
        assert_eq!(created.get("qwen/qwen4-max"), Some(&1794795526));
        assert!(!created.contains_key("plain-model-2"));
        assert_eq!(strip_vendor("qwen/qwen4-max"), "qwen4-max");
        assert_eq!(strip_vendor("plain-model-2"), "plain-model-2");
        assert!(parse_catalog("not json").ids.is_empty());
        assert!(parse_catalog(r#"{"data":"x"}"#).ids.is_empty());
    }

    /// (0.2.5 D3) A catalog entry's `pricing.prompt` / `pricing.completion` are kept per id when
    /// they are positive numbers (a decimal string or a number); a free (`"0"`), router (`"-1"`) or
    /// unparsable price is none, and an entry with neither field carries no price at all.
    #[test]
    fn the_catalog_parser_keeps_positive_list_prices_only() {
        let body = r#"{"data":[
            {"id":"qwen/qwen4-max","pricing":{"prompt":"0.000003","completion":"0.000012","image":"0.5"}},
            {"id":"qwen/qwen3.8-max","pricing":{"prompt":0.000001,"completion":"x"}},
            {"id":"openrouter/auto","pricing":{"prompt":"-1","completion":"-1"}},
            {"id":"acme/free-1","pricing":{"prompt":"0","completion":"0"}},
            {"id":"acme/none-1"}
        ]}"#;
        let s = parse_catalog(body);
        assert_eq!(s.ids.len(), 5);
        assert_eq!(
            s.prices.get("qwen/qwen4-max"),
            Some(&ListPrice {
                prompt: Some(0.000003),
                completion: Some(0.000012)
            })
        );
        assert_eq!(
            s.prices.get("qwen/qwen3.8-max"),
            Some(&ListPrice {
                prompt: Some(0.000001),
                completion: None
            })
        );
        assert_eq!(s.prices.len(), 2, "{:?}", s.prices);
        // prices live for the pass only: the state file never holds them
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            !json.contains("pricing") && !json.contains("prices"),
            "{json}"
        );
        assert!(!json.contains("0.000003"), "{json}");
    }

    #[test]
    fn the_catalog_parser_caps_size_and_count() {
        // the count cap: at most MAX_IDS ids
        let many: Vec<String> = (0..MAX_IDS + 50)
            .map(|i| format!("{{\"id\":\"acme/m-{i}\"}}"))
            .collect();
        let body = format!("{{\"data\":[{}]}}", many.join(","));
        assert_eq!(parse_catalog(&body).ids.len(), MAX_IDS);
        // the size cap: a body past BODY_CAP is dropped whole
        let pad = " ".repeat(BODY_CAP + 1);
        let body = format!("{{\"data\":[{{\"id\":\"acme/m-1\"}}]}}{pad}");
        assert!(parse_catalog(&body).ids.is_empty());
        let re = regex::Regex::new(r"(m-\d+)").unwrap();
        assert!(parse_page(&body, &re).is_empty());
    }

    #[test]
    fn the_page_parser_takes_group_one_and_filters() {
        let re = regex::Regex::new(r#"<code>([^<]+)</code>"#).unwrap();
        let body = "<code>qwen4-max</code> <code>qwen4-max</code> <code>bad id</code> <code>qwen4-plus</code>";
        assert_eq!(parse_page(body, &re), vec!["qwen4-max", "qwen4-plus"]);
    }

    #[test]
    fn a_catalog_is_fetched_keyless_and_recorded_by_its_url() {
        let env = FakeHttp {
            body: r#"{"data":[{"id":"qwen/qwen4-max","created":1}]}"#.into(),
            status: 200,
            seen: Default::default(),
        };
        let out = collect(
            &env,
            &["https://openrouter.ai/api/v1/models".to_string()],
            &[],
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, "web:https://openrouter.ai:443/api/v1/models");
        assert_eq!(out[0].ids, vec!["qwen/qwen4-max"]);
        let seen = env.seen.borrow();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].1, 0, "no header (no key) is ever attached");
        // (F02-12, F02-13) the public policy: same origin only, one 15 s deadline, the 4 MiB cap
        // enforced by the reader
        assert_eq!(
            seen[0].2,
            GetPolicy {
                timeout: Duration::from_secs(15),
                redirects: Redirects::SameOrigin,
                cap: BODY_CAP,
            }
        );
        // an http catalog is never fetched; a failing status yields nothing
        drop(seen);
        let env2 = FakeHttp {
            body: String::new(),
            status: 500,
            seen: Default::default(),
        };
        assert!(collect(&env2, &["http://example.com/models".to_string()], &[]).is_empty());
        assert!(env2.seen.borrow().is_empty());
        assert!(collect(&env2, &["https://example.com/models".to_string()], &[]).is_empty());
    }

    /// A fake that answers per URL (an unmapped URL is a 404) and records what was asked.
    struct MapHttp {
        bodies: Vec<(&'static str, String)>,
        seen: std::cell::RefCell<Vec<String>>,
    }
    impl Env for MapHttp {
        fn run(&self, _p: &str, _a: &[&str], _e: &[(&str, &str)], _c: Option<&Path>) -> RunResult {
            RunResult::default()
        }
        fn http_get(
            &self,
            url: &str,
            _headers: &[(&str, &str)],
            _policy: GetPolicy,
        ) -> Result<(u16, String), String> {
            self.seen.borrow_mut().push(url.to_string());
            Ok(self
                .bodies
                .iter()
                .find(|(u, _)| *u == url)
                .map(|(_, b)| (200, b.clone()))
                .unwrap_or((404, String::new())))
        }
    }

    /// (F16-1, F17-5) A catalog's identity is its full URL: scheme, host, port, path and query.
    #[test]
    fn the_catalog_source_name_is_the_canonical_url() {
        assert_eq!(
            catalog_source("https://Models.Example/v1/models?tier=pub").as_deref(),
            Some("web:https://models.example:443/v1/models?tier=pub")
        );
        assert_eq!(
            catalog_source("https://models.example:443/v1/models"),
            catalog_source("https://MODELS.example/v1/models")
        );
        for (a, b) in [
            ("https://m.example/a", "https://m.example/b"),
            ("https://m.example/a", "https://m.example:8443/a"),
            ("https://m.example/a", "https://m.example/a?x=1"),
        ] {
            assert_ne!(catalog_source(a), catalog_source(b), "{a} / {b}");
        }
        assert!(catalog_source("http://m.example/a").is_none());
        assert!(is_legacy_catalog_source("web:openrouter.ai"));
        assert!(!is_legacy_catalog_source(
            "web:https://openrouter.ai:443/api/v1/models"
        ));
        assert!(!is_legacy_catalog_source("page:zai-docs"));
        assert_eq!(
            catalog_source_host("web:https://openrouter.ai:443/api/v1/models").as_deref(),
            Some("openrouter.ai")
        );
        assert_eq!(
            catalog_source_host("web:OpenRouter.ai").as_deref(),
            Some("openrouter.ai")
        );
    }

    /// A catalog's name is SHOWN without its query (which may hold a token): scheme, host, the port
    /// only when it is not 443, the path, and ` (query not shown)` when a query was dropped; every
    /// other source name is shown as it is.
    #[test]
    fn a_catalog_source_is_shown_without_its_query() {
        let shown = |url: &str| catalog_source_display(&catalog_source(url).unwrap());
        assert_eq!(
            shown("https://openrouter.ai/api/v1/models"),
            "web:https://openrouter.ai/api/v1/models"
        );
        let s = shown("https://models.example/v1/models?key=sk-SECRET-1&tier=pub");
        assert_eq!(s, "web:https://models.example/v1/models (query not shown)");
        assert!(!s.contains("SECRET") && !s.contains("key="), "{s}");
        assert_eq!(
            shown("https://models.example:8443/v1/models?token=abc"),
            "web:https://models.example:8443/v1/models (query not shown)"
        );
        assert_eq!(
            shown("https://models.example:443/m"),
            "web:https://models.example/m"
        );
        // an empty query drops nothing
        assert_eq!(
            shown("https://models.example/m?"),
            "web:https://models.example/m"
        );
        // the stored key keeps the full URL
        assert_eq!(
            catalog_source("https://models.example/v1/models?key=sk-SECRET-1").as_deref(),
            Some("web:https://models.example:443/v1/models?key=sk-SECRET-1")
        );
        // other names as they are; a key that does not parse is cut at the query by text
        for name in ["web:openrouter.ai", "page:zai-docs", "grok"] {
            assert_eq!(catalog_source_display(name), name);
        }
        assert_eq!(
            catalog_source_display("web:https://[bad/x?key=SECRET"),
            "web:https://[bad/x (query not shown)"
        );
    }

    /// (F16-1, F17-5) Two catalogs on one host, the private one listed FIRST: both are fetched,
    /// each recorded under its own URL, and only the URL declared `"public": true` is trusted - the
    /// private catalog's id is never a published name (it used to be the only one fetched for the
    /// host, and the host was trusted because its other URL was public).
    #[test]
    fn two_catalogs_on_one_host_are_two_sources_and_only_the_declared_one_is_trusted() {
        let private = "https://models.example/private/v1/models";
        let public = "https://models.example/public/v1/models";
        let env = MapHttp {
            bodies: vec![
                (
                    private,
                    r#"{"data":[{"id":"z-ai/glm-9-private"}]}"#.to_string(),
                ),
                (
                    public,
                    r#"{"data":[{"id":"z-ai/glm-9-public"}]}"#.to_string(),
                ),
            ],
            seen: Default::default(),
        };
        let out = collect(&env, &[private.to_string(), public.to_string()], &[]);
        assert_eq!(
            *env.seen.borrow(),
            vec![private, public],
            "both are fetched"
        );
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[0].source,
            "web:https://models.example:443/private/v1/models"
        );
        assert_eq!(out[0].ids, vec!["z-ai/glm-9-private"]);
        assert_eq!(
            out[1].source,
            "web:https://models.example:443/public/v1/models"
        );
        let cfg = crate::c3config::C3Config {
            model_catalogs: Some(vec![
                crate::c3config::ModelCatalogConfig {
                    url: private.into(),
                    public: false,
                },
                crate::c3config::ModelCatalogConfig {
                    url: public.into(),
                    public: true,
                },
            ]),
            ..Default::default()
        };
        let trusted = trusted_sources(Some(&cfg));
        assert!(trusted.iter().any(|t| t.source == out[1].source));
        assert!(!trusted.iter().any(|t| t.source == out[0].source));
        let state = crate::maintain::models::ModelsState {
            sources: out,
            ..Default::default()
        };
        let published = crate::maintain::models::published_from(&state, &trusted);
        assert!(published.contains(&("zai".to_string(), "glm-9-public".to_string())));
        assert!(
            !published.iter().any(|(_, m)| m == "glm-9-private"),
            "{published:?}"
        );
        // another URL on the BUILT-IN catalog's host is not trusted by the built-in entry
        let builtin = trusted_sources(None);
        assert_eq!(builtin.len(), 1);
        assert_eq!(
            builtin[0].source,
            "web:https://openrouter.ai:443/api/v1/models"
        );
        assert_ne!(
            catalog_source("https://openrouter.ai/api/v1/models?private=1"),
            Some(builtin[0].source.clone())
        );
    }

    /// (F15-1) A catalog or page on an IP literal or `localhost` is never requested.
    #[test]
    fn a_public_source_on_an_ip_literal_or_localhost_is_never_fetched() {
        let env = MapHttp {
            bodies: Vec::new(),
            seen: Default::default(),
        };
        let page = |url: &str| Page {
            name: format!("p-{}", url.len()),
            url: url.into(),
            pattern: regex::Regex::new(r"(m-\d+)").unwrap(),
        };
        let out = collect(
            &env,
            &[
                "https://127.0.0.1/models".to_string(),
                "https://[::1]/models".to_string(),
                "https://LocalHost./models".to_string(),
            ],
            &[
                page("https://192.168.1.10/models"),
                page("https://localhost/m"),
            ],
        );
        assert!(out.is_empty());
        assert!(env.seen.borrow().is_empty(), "{:?}", env.seen.borrow());
    }

    #[test]
    fn a_page_source_is_named_by_its_label() {
        let env = FakeHttp {
            body: "model: glm-6 and glm-6-air".into(),
            status: 200,
            seen: Default::default(),
        };
        let page = Page {
            name: "zai-docs".into(),
            url: "https://docs.z.ai/models".into(),
            pattern: regex::Regex::new(r"\b(glm-[0-9][a-z0-9.-]*)").unwrap(),
        };
        let out = collect(&env, &[], &[page]);
        assert_eq!(out[0].source, "page:zai-docs");
        assert_eq!(out[0].ids, vec!["glm-6", "glm-6-air"]);
    }
}
