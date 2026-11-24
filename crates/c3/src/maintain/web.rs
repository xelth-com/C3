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
//! default catalog list, `"model_pages"` adds pages. Every recorded list carries the URL it was read
//! from and whether that URL was trusted as public then ([`provenance`]).

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
/// every capture is a candidate id. (F57-1, F62-3) `public` is whether the page's exact URL is
/// trusted as public ([`trusted_sources`]) at the time it is read - recorded with its ids
/// ([`SourceList::public`]).
#[derive(Debug, Clone)]
pub struct Page {
    pub name: String,
    pub url: String,
    pub pattern: regex::Regex,
    pub public: bool,
}

/// (F62-3) A catalog to fetch: its URL and whether that exact URL is trusted as public
/// ([`trusted_sources`]: a catalog shipped in the code, or a config entry declaring it `"public":
/// true`) at the time it is read - recorded with its ids ([`SourceList::public`]), exactly as for a
/// [`Page`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    pub url: String,
    pub public: bool,
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
/// load - the same reading telemetry's [`trusted_sources`] gets), through [`sources_of`].
pub fn configured_sources() -> (Vec<Catalog>, Vec<Page>) {
    sources_of(crate::c3config::load().ok().flatten().as_ref())
}

/// The catalogs and pages of `cfg` (the defaults for `None`: the built-in catalogs, no page), each
/// stamped (F57-1, F62-3) with whether its exact canonical URL is trusted as public NOW
/// ([`trusted_sources`] of the same `cfg`) - the collection-time status [`collect`] records with its
/// ids. ONE rule for a catalog and a page: the trust that later publishes a record's ids is the
/// trust that decided, when they were read, what the record may claim.
pub fn sources_of(cfg: Option<&crate::c3config::C3Config>) -> (Vec<Catalog>, Vec<Page>) {
    let trusted = trusted_sources(cfg);
    let is_trusted =
        |key: Option<String>| key.is_some_and(|k| trusted.iter().any(|t| t.source == k));
    let catalog = |url: &str| Catalog {
        url: url.to_string(),
        public: is_trusted(catalog_source(url)),
    };
    let builtin = || -> Vec<Catalog> { DEFAULT_CATALOGS.iter().map(|u| catalog(u)).collect() };
    let Some(cfg) = cfg else {
        return (builtin(), Vec::new());
    };
    let catalogs = match &cfg.model_catalogs {
        Some(list) => list.iter().map(|c| catalog(&c.url)).collect(),
        None => builtin(),
    };
    let pages = cfg
        .model_pages
        .iter()
        .filter_map(|p| {
            regex::Regex::new(&p.pattern).ok().map(|re| Page {
                name: p.name.clone(),
                url: p.url.clone(),
                pattern: re,
                public: is_trusted(page_key(&p.url)),
            })
        })
        .collect();
    (catalogs, pages)
}

/// (F16-1, F17-5, F57-1) The canonical form of a public source's URL - the ONE rule by which a
/// catalog and a page are identified, recorded and trusted: `https://` only, the lower-case host,
/// the port (the scheme's default when none is written), the path and the query
/// (`https://openrouter.ai:443/api/v1/models`); the fragment is dropped. `None` for anything else.
pub fn canonical_url(url: &str) -> Option<String> {
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
        "https://{host}:{}{}{query}",
        u.port_or_known_default().unwrap_or(443),
        u.path()
    ))
}

/// (F16-1, F17-5) The source name of a catalog: `web:<url>` - its FULL canonical URL
/// ([`canonical_url`]), the identity of the source (`web:https://openrouter.ai:443/api/v1/models`).
/// Two catalogs on one host are two sources, each fetched and recorded under its own name, each
/// trusted only by its own config entry.
pub fn catalog_source(url: &str) -> Option<String> {
    canonical_url(url).map(|u| format!("web:{u}"))
}

/// (F57-1) The trust key of a page: `page:` + its canonical URL ([`canonical_url`]) - never its
/// label, which a later configuration may give to another URL.
pub fn page_key(url: &str) -> Option<String> {
    canonical_url(url).map(|u| format!("page:{u}"))
}

/// (F57-1, F62-3) The provenance of a recorded source: the key [`trusted_sources`] must hold for its
/// ids to be published, or `None` when they never are. ONE rule for both public kinds, read from
/// the same two fields [`collect`] writes for both: the record must say that its URL was trusted
/// as public when the ids were read ([`SourceList::public`]), and the key is that URL
/// ([`SourceList::url`]) in its canonical form ([`canonical_url`]) under the kind's prefix - a
/// catalog's `web:<url>` (which must equal the record's name, F16-1: a record whose URL disagrees
/// with its name is withheld), a page's `page:<url>` ([`page_key`]), whatever its label. So a URL
/// declared public only later never lends its trust to ids read while it was not (F62-3), and a
/// label reused for another URL never lends its trust to ids read from the earlier one (F57-1).
///
/// A record of an earlier build carries no collection-time status (no `url`, no `public`): it has
/// none and is withheld until the next refresh rewrites it - a page record (label only), a custom
/// catalog record, a legacy host-keyed catalog record (`web:<host>`,
/// [`is_legacy_catalog_source`]). The ONE exception is the record of a catalog shipped in the code
/// ([`is_builtin_catalog_source`]: its name is exactly a [`DEFAULT_CATALOGS`] URL in canonical
/// form): that URL's trust never came from the configuration - every build trusted it by code and
/// read it keyless - so it was public when its ids were read, and its key is its name. An engine's
/// or an endpoint's own list: none.
pub fn provenance(s: &SourceList) -> Option<String> {
    let kind = if s.source.starts_with("web:") {
        "web"
    } else if s.source.starts_with("page:") {
        "page"
    } else {
        return None;
    };
    if s.public {
        let key = format!("{kind}:{}", canonical_url(s.url.as_deref()?)?);
        return (kind == "page" || key == s.source).then_some(key);
    }
    (s.url.is_none() && is_builtin_catalog_source(&s.source)).then(|| s.source.clone())
}

/// (F62-3) Whether a source name is exactly the record name ([`catalog_source`]) of a catalog
/// shipped in the code ([`DEFAULT_CATALOGS`]) - by the full canonical URL, never by host: another
/// URL on the built-in catalog's host, or a legacy host-keyed `web:openrouter.ai`, is not it.
pub fn is_builtin_catalog_source(source: &str) -> bool {
    DEFAULT_CATALOGS
        .iter()
        .any(|u| catalog_source(u).as_deref() == Some(source))
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
    /// (F57-1) The trust key a record's [`provenance`] must equal: a catalog's `web:<url>`, a
    /// page's `page:<url>` ([`page_key`]) - by canonical URL, never by a page's label.
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
/// ([`is_legacy_catalog_source`]) matches no trusted source. (F57-1) A page is trusted by its
/// canonical URL too ([`page_key`]) - the same rule, so the two kinds cannot drift apart.
/// (F62-3) Today's trust alone publishes nothing: a record's ids count only when the record also
/// says its URL was trusted as public when they were read ([`provenance`]) - the same function,
/// applied at collection ([`sources_of`]), decided that.
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
            if let Some(key) = page_key(&p.url) {
                add(key, &p.url);
            }
        }
    }
    out
}

/// The model part of a catalog id (`qwen/qwen4-max` -> `qwen4-max`); a plain id is itself.
pub fn strip_vendor(id: &str) -> &str {
    id.split_once('/').map(|(_, m)| m).unwrap_or(id)
}

/// Collect every configured public source: (F16-1, F17-5) every distinct catalog URL under its own
/// name ([`catalog_source`]), every page under its label - each (F57-1, F62-3) with the canonical
/// URL it was read from and whether that URL was trusted as public then ([`Catalog::public`],
/// [`Page::public`]), recorded with its ids by one function ([`recorded`]) and read back by one rule
/// ([`provenance`]). A source that fails (network, status, cap, parse) is left out; a source with no
/// valid id is left out.
pub fn collect(env: &dyn Env, catalogs: &[Catalog], pages: &[Page]) -> Vec<SourceList> {
    let mut out: Vec<SourceList> = Vec::new();
    let mut fetched: Vec<String> = Vec::new();
    for c in catalogs {
        let (Some(source), Some(url)) = (catalog_source(&c.url), canonical_url(&c.url)) else {
            continue;
        };
        if fetched.contains(&source) {
            continue;
        }
        fetched.push(source.clone());
        let Some(body) = keyless_get(env, &c.url) else {
            continue;
        };
        let list = parse_catalog(&body);
        if !list.ids.is_empty() {
            out.push(recorded(source, url, c.public, list));
        }
    }
    for p in pages {
        let source = format!("page:{}", p.name);
        if out.iter().any(|s| s.source == source) {
            continue;
        }
        let Some(url) = canonical_url(&p.url) else {
            continue;
        };
        let Some(body) = keyless_get(env, &p.url) else {
            continue;
        };
        let ids = parse_page(&body, &p.pattern);
        if !ids.is_empty() {
            let list = SourceList {
                ids,
                ..Default::default()
            };
            out.push(recorded(source, url, p.public, list));
        }
    }
    out
}

/// (F57-1, F62-3) A collected list as it is recorded - ONE shape for a catalog and a page: under
/// `source`, with the canonical `url` its ids were read from and whether that URL was trusted as
/// public at that moment ([`sources_of`]); [`provenance`] reads exactly these two fields back.
fn recorded(source: String, url: String, public: bool, list: SourceList) -> SourceList {
    SourceList {
        source,
        url: Some(url),
        public,
        ..list
    }
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
        let (catalogs, pages) = sources_of(None);
        assert_eq!(
            catalogs,
            vec![cat("https://openrouter.ai/api/v1/models", true)],
            "the built-in catalog, trusted by the code"
        );
        let out = collect(&env, &catalogs, &pages);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, "web:https://openrouter.ai:443/api/v1/models");
        assert_eq!(out[0].ids, vec!["qwen/qwen4-max"]);
        // (F62-3) recorded with the URL it was read from and that it was public then
        assert_eq!(
            out[0].url.as_deref(),
            Some("https://openrouter.ai:443/api/v1/models")
        );
        assert!(out[0].public);
        assert_eq!(
            provenance(&out[0]).as_deref(),
            Some("web:https://openrouter.ai:443/api/v1/models")
        );
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
        assert!(collect(&env2, &[cat("http://example.com/models", true)], &[]).is_empty());
        assert!(env2.seen.borrow().is_empty());
        assert!(collect(&env2, &[cat("https://example.com/models", true)], &[]).is_empty());
    }

    /// A catalog to collect, with the public status it is read under.
    fn cat(url: &str, public: bool) -> Catalog {
        Catalog {
            url: url.into(),
            public,
        }
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
        let (catalogs, pages) = sources_of(Some(&cfg));
        assert_eq!(catalogs, vec![cat(private, false), cat(public, true)]);
        let out = collect(&env, &catalogs, &pages);
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
        assert!(!out[0].public, "read while not declared public");
        assert_eq!(
            out[1].source,
            "web:https://models.example:443/public/v1/models"
        );
        assert!(out[1].public);
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
            public: true,
        };
        let out = collect(
            &env,
            &[
                cat("https://127.0.0.1/models", true),
                cat("https://[::1]/models", true),
                cat("https://LocalHost./models", true),
            ],
            &[
                page("https://192.168.1.10/models"),
                page("https://localhost/m"),
            ],
        );
        assert!(out.is_empty());
        assert!(env.seen.borrow().is_empty(), "{:?}", env.seen.borrow());
    }

    /// A page is named by its label; (F57-1) its record carries the canonical URL it was read from
    /// and whether that URL was declared public then - its provenance.
    #[test]
    fn a_page_source_is_named_by_its_label() {
        let env = FakeHttp {
            body: "model: glm-6 and glm-6-air".into(),
            status: 200,
            seen: Default::default(),
        };
        let page = Page {
            name: "zai-docs".into(),
            url: "https://Docs.Z.ai/models#list".into(),
            pattern: regex::Regex::new(r"\b(glm-[0-9][a-z0-9.-]*)").unwrap(),
            public: true,
        };
        let out = collect(&env, &[], &[page]);
        assert_eq!(out[0].source, "page:zai-docs");
        assert_eq!(out[0].ids, vec!["glm-6", "glm-6-air"]);
        assert_eq!(out[0].url.as_deref(), Some("https://docs.z.ai:443/models"));
        assert!(out[0].public);
        assert_eq!(
            provenance(&out[0]).as_deref(),
            Some("page:https://docs.z.ai:443/models")
        );
    }

    /// (F57-1, the reviewer's RC1 at collection level) Ids read from a page that was NOT declared
    /// public are recorded as such; when a later configuration reuses the page's label for another
    /// URL declared public, the trusted key is that URL and the earlier record matches nothing.
    /// After a refresh from the public URL the same label's record is published.
    #[test]
    fn a_reused_page_label_never_lends_its_trust_to_ids_of_the_earlier_url() {
        let internal = "https://wiki.acmecorp-internal.example/llm";
        let public = "https://docs.z.ai/models";
        let env = MapHttp {
            bodies: vec![
                (internal, "model glm-9-private".to_string()),
                (public, "model glm-9-private and glm-9-public".to_string()),
            ],
            seen: Default::default(),
        };
        let pattern = regex::Regex::new(r"\b(glm-[0-9][a-z0-9.-]*)").unwrap();
        let earlier = Page {
            name: "docs".into(),
            url: internal.into(),
            pattern: pattern.clone(),
            public: false,
        };
        let recorded = collect(&env, &[], &[earlier]);
        assert_eq!(recorded[0].source, "page:docs");
        assert!(!recorded[0].public);
        assert_eq!(provenance(&recorded[0]), None, "read while not public");
        let state = crate::maintain::models::ModelsState {
            sources: recorded,
            ..Default::default()
        };
        let cfg = crate::c3config::C3Config {
            model_pages: vec![crate::c3config::ModelPageConfig {
                name: "docs".into(),
                url: public.into(),
                pattern: pattern.as_str().into(),
                public: true,
            }],
            ..Default::default()
        };
        let trusted = trusted_sources(Some(&cfg));
        let published = crate::maintain::models::published_from(&state, &trusted);
        assert!(published.is_empty(), "{published:?}");
        // the refresh from the public URL rewrites the record: now published
        let refreshed = collect(
            &env,
            &[],
            &[Page {
                name: "docs".into(),
                url: public.into(),
                pattern,
                public: true,
            }],
        );
        let state = crate::maintain::models::ModelsState {
            sources: refreshed,
            ..Default::default()
        };
        let published = crate::maintain::models::published_from(&state, &trusted);
        assert!(published.contains(&("zai".to_string(), "glm-9-public".to_string())));
        assert!(
            published.contains(&("zai".to_string(), "glm-9-private".to_string())),
            "the public page lists it itself now: {published:?}"
        );
    }

    /// (F62-3, the reviewer's RC3 at collection level) A catalog read while its URL was NOT
    /// declared public, its cache holding a private id; the configuration then declares the SAME URL
    /// public and the old cache is kept: nothing of that cache is published - the record says the
    /// URL was not public when the ids were read (before the fix, today's trust of the URL was
    /// enough). A refresh of the cleaned catalog rewrites the record: its public id is published,
    /// the private one - no longer listed - is not. A custom catalog's record of an earlier build
    /// (no status) is withheld as well.
    #[test]
    fn a_catalog_declared_public_later_never_publishes_ids_cached_while_it_was_private() {
        let url = "https://models.example/v1/models";
        let configured = |public: bool| crate::c3config::C3Config {
            model_catalogs: Some(vec![crate::c3config::ModelCatalogConfig {
                url: url.into(),
                public,
            }]),
            ..Default::default()
        };
        let published = |sources: Vec<SourceList>, cfg: &crate::c3config::C3Config| {
            crate::maintain::models::published_from(
                &crate::maintain::models::ModelsState {
                    sources,
                    ..Default::default()
                },
                &trusted_sources(Some(cfg)),
            )
        };
        // 1. read while not public: the private id is cached with that status
        let private_env = MapHttp {
            bodies: vec![(
                url,
                r#"{"data":[{"id":"z-ai/glm-9-private"},{"id":"z-ai/glm-9-public"}]}"#.to_string(),
            )],
            seen: Default::default(),
        };
        let (catalogs, pages) = sources_of(Some(&configured(false)));
        assert_eq!(catalogs, vec![cat(url, false)]);
        let cached = collect(&private_env, &catalogs, &pages);
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].source, "web:https://models.example:443/v1/models");
        assert_eq!(
            cached[0].url.as_deref(),
            Some("https://models.example:443/v1/models")
        );
        assert!(!cached[0].public);
        assert_eq!(provenance(&cached[0]), None, "read while not public");
        assert!(published(cached.clone(), &configured(false)).is_empty());
        // 2. the same URL declared public, the old cache kept: still nothing
        let public_cfg = configured(true);
        assert!(trusted_sources(Some(&public_cfg))
            .iter()
            .any(|t| t.source == cached[0].source));
        let p = published(cached, &public_cfg);
        assert!(p.is_empty(), "{p:?}");
        // 3. a refresh of the cleaned catalog: its public id is published, the private one is not
        let clean_env = MapHttp {
            bodies: vec![(url, r#"{"data":[{"id":"z-ai/glm-9-public"}]}"#.to_string())],
            seen: Default::default(),
        };
        let (catalogs, pages) = sources_of(Some(&public_cfg));
        assert_eq!(catalogs, vec![cat(url, true)]);
        let refreshed = collect(&clean_env, &catalogs, &pages);
        assert_eq!(refreshed.len(), 1);
        assert!(refreshed[0].public);
        let p = published(refreshed, &public_cfg);
        assert!(
            p.contains(&("zai".to_string(), "glm-9-public".to_string())),
            "{p:?}"
        );
        assert!(!p.iter().any(|(_, m)| m == "glm-9-private"), "{p:?}");
        // a custom catalog's record of an earlier build (no status): withheld until refreshed
        let legacy: crate::maintain::models::ModelsState = serde_json::from_str(
            r#"{"sources":[{"source":"web:https://models.example:443/v1/models","ids":["z-ai/glm-9-public"]}]}"#,
        )
        .unwrap();
        assert_eq!(provenance(&legacy.sources[0]), None);
        assert!(published(legacy.sources, &public_cfg).is_empty());
    }
}
