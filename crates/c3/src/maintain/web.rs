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

use std::collections::BTreeMap;
use std::time::Duration;

use super::env::{Env, GetPolicy, Redirects};
use super::models::SourceList;
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
            let catalogs = cfg
                .model_catalogs
                .clone()
                .unwrap_or_else(|| DEFAULT_CATALOGS.iter().map(|s| s.to_string()).collect());
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

/// The source name of a catalog: `web:<host>`.
pub fn catalog_source(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    if u.scheme() != "https" {
        return None;
    }
    let host = u.host_str()?.to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    Some(format!("web:{host}"))
}

/// Whether a source name is a public one (a catalog or a page), whose ids are published names.
pub fn is_public_source(source: &str) -> bool {
    source.starts_with("web:") || source.starts_with("page:")
}

/// The model part of a catalog id (`qwen/qwen4-max` -> `qwen4-max`); a plain id is itself.
pub fn strip_vendor(id: &str) -> &str {
    id.split_once('/').map(|(_, m)| m).unwrap_or(id)
}

/// Collect every configured public source. A source that fails (network, status, cap, parse) is
/// left out; a source with no valid id is left out.
pub fn collect(env: &dyn Env, catalogs: &[String], pages: &[Page]) -> Vec<SourceList> {
    let mut out: Vec<SourceList> = Vec::new();
    for url in catalogs {
        let Some(source) = catalog_source(url) else {
            continue;
        };
        if out.iter().any(|s| s.source == source) {
            continue;
        }
        let Some(body) = keyless_get(env, url) else {
            continue;
        };
        let (ids, created) = parse_catalog(&body);
        if !ids.is_empty() {
            out.push(SourceList {
                source,
                ids,
                created,
            });
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
                created: BTreeMap::new(),
            });
        }
    }
    out
}

/// A keyless GET of a public source: https only, no header but the client's own generic
/// User-Agent (`c3/<version>`), under [`PUBLIC_GET`]. `None` on any failure.
///
/// Under `CODEX_CONSULT_TEST_MODE` no request of the real [`SystemEnv`](super::env::SystemEnv)
/// reaches the network: `C3_MAINTAIN_TEST_HTTP`
/// (`<url>|<file>;...`) maps a URL to a local file, an unmapped URL fails, and every requested URL
/// is appended to `C3_MAINTAIN_TEST_HTTP_LOG` when set - so the integration tests can both feed a
/// catalog and prove that a switched-off pass fetched nothing.
fn keyless_get(env: &dyn Env, url: &str) -> Option<String> {
    if !url.starts_with("https://") {
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
/// deadline over every hop; the reader stops at [`BODY_CAP`] + 1 bytes.
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

/// Parse a catalog body (`{"data":[{"id":"<vendor>/<model>","created":<unix secs>}, ...]}`) into
/// its ids and their `created` stamps. An id with a `:variant` suffix (`:free`, `:batch`) is
/// skipped; the vendor part must be a lower-case slug and the model part must pass
/// [`valid_model_id`] (with no further `/`); a plain id without a vendor is kept when it passes.
/// At most [`MAX_IDS`] ids, de-duplicated, in catalog order.
pub fn parse_catalog(body: &str) -> (Vec<String>, BTreeMap<String, i64>) {
    let mut ids: Vec<String> = Vec::new();
    let mut created: BTreeMap<String, i64> = BTreeMap::new();
    if body.len() > BODY_CAP {
        return (ids, created);
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return (ids, created);
    };
    let Some(arr) = v.get("data").and_then(|d| d.as_array()) else {
        return (ids, created);
    };
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
        ids.push(id);
    }
    (ids, created)
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
        let (ids, created) = parse_catalog(body);
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
        assert!(parse_catalog("not json").0.is_empty());
        assert!(parse_catalog(r#"{"data":"x"}"#).0.is_empty());
    }

    #[test]
    fn the_catalog_parser_caps_size_and_count() {
        // the count cap: at most MAX_IDS ids
        let many: Vec<String> = (0..MAX_IDS + 50)
            .map(|i| format!("{{\"id\":\"acme/m-{i}\"}}"))
            .collect();
        let body = format!("{{\"data\":[{}]}}", many.join(","));
        assert_eq!(parse_catalog(&body).0.len(), MAX_IDS);
        // the size cap: a body past BODY_CAP is dropped whole
        let pad = " ".repeat(BODY_CAP + 1);
        let body = format!("{{\"data\":[{{\"id\":\"acme/m-1\"}}]}}{pad}");
        assert!(parse_catalog(&body).0.is_empty());
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
    fn a_catalog_is_fetched_keyless_and_recorded_by_host() {
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
        assert_eq!(out[0].source, "web:openrouter.ai");
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
