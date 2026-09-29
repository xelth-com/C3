//! Milestone 9 router tests: v1/R15 parity, priors validation (fail closed), ed25519
//! signature verification, the offline download (no network), and the ledger replay against
//! a real routed fixture.
//!
//! Nothing here touches the network: the download is driven through an injected [`Fetcher`]
//! that records whether it was called, and the switch tests assert it was NOT.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use chrono::Utc;

use base64::Engine as _;
use c3::panel::routing::{routing_score, Rating};
use c3::router::download::{self, Fetched, Fetcher, RefreshOutcome};
use c3::router::{priors::Priors, priors::PriorsError, score, sign, Lineage, RouterContext};
use c3_core::ledger::SessionsFile;
use ed25519_dalek::{Signer, SigningKey};

// --------------------------------------------------------------------------- env serialisation

/// Environment reads (`resolve_mode`, `C3_PRIORS_KEY`) are process-global; serialise every
/// env-touching test through one lock and restore the prior values on drop.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

struct EnvGuard {
    keys: Vec<(&'static str, Option<String>)>,
}
impl EnvGuard {
    fn set(vars: &[(&'static str, Option<&str>)]) -> Self {
        let keys = vars
            .iter()
            .map(|(k, v)| {
                let prev = std::env::var(k).ok();
                match v {
                    Some(val) => std::env::set_var(k, val),
                    None => std::env::remove_var(k),
                }
                (*k, prev)
            })
            .collect();
        EnvGuard { keys }
    }
}
impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, prev) in &self.keys {
            match prev {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
}

static TMP: AtomicUsize = AtomicUsize::new(0);
fn temp_dir(tag: &str) -> PathBuf {
    let n = TMP.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("c3-router-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// --------------------------------------------------------------------------- (1) v1/R15 parity

#[test]
fn v1_without_priors_matches_routing_score_bit_for_bit() {
    // A tiny xorshift PRNG — no `rand` dependency; the repo derives its own uniforms.
    let mut s: u64 = 0xda3e39cb94b95bdb;
    let mut next = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s >> 11) as f64 / (1u64 << 53) as f64
    };
    let now = Utc::now();
    let cw = Some(now.fixed_offset());
    let ctx = RouterContext::default(); // no priors, default params
    let usefuls = ["yes", "partly", "no"];
    let purposes = ["framing", "decision", "chore"];
    let topics_pool = ["api", "perf", "sec", ""];

    for _ in 0..2000 {
        let count = (next() * 12.0) as usize;
        let ratings: Vec<Rating> = (0..count)
            .map(|_| {
                let topics: Vec<String> = {
                    let t = topics_pool[(next() * topics_pool.len() as f64) as usize];
                    if t.is_empty() {
                        vec![]
                    } else {
                        vec![t.to_string()]
                    }
                };
                Rating {
                    provider: "openai".into(),
                    model: "gpt-6".into(),
                    engine: "codex".into(),
                    purpose: purposes[(next() * purposes.len() as f64) as usize].into(),
                    topics,
                    consult_when: cw,
                    useful: usefuls[(next() * usefuls.len() as f64) as usize].into(),
                }
            })
            .collect();

        let want_topics = vec!["api".to_string()];
        let base = routing_score(
            &ratings,
            "openai",
            "gpt-6",
            "codex",
            "framing",
            &want_topics,
            now,
            false,
        );
        let got = score(
            &ratings,
            Lineage {
                provider: "openai",
                model: "gpt-6",
                engine: "codex",
            },
            "framing",
            &want_topics,
            now,
            &ctx,
        );
        assert_eq!(
            got.base.score.to_bits(),
            base.score.to_bits(),
            "score differs: basis={} n={count}",
            base.basis
        );
        assert_eq!(got.base.basis, base.basis);
        assert!(
            got.prior.is_none(),
            "no ext with no priors and default params"
        );
    }
}

// --------------------------------------------------------------------------- (2) priors validation

fn priors_bytes(version: i64, cells: &str) -> Vec<u8> {
    format!(r#"{{"priors_version":{version},"generated":"2026-09-01T00:00:00Z","window_days":90,"cells":[{cells}]}}"#).into_bytes()
}

#[test]
fn priors_validation_fails_closed() {
    // A good file with an unknown key ignored.
    let ok = priors_bytes(
        1,
        r#"{"provider":"openai","model":"gpt-6","engine":"codex","purpose":"decision","topic":"","mean":0.71,"n":42,"unknown":"ignored"}"#,
    );
    assert!(Priors::validate(&ok).is_ok());

    // Wrong version.
    assert!(matches!(
        Priors::validate(&priors_bytes(2, "")),
        Err(PriorsError::BadVersion(2))
    ));
    // Not JSON.
    assert!(matches!(
        Priors::validate(b"not json"),
        Err(PriorsError::Parse)
    ));
    // Over 1 MiB.
    let big = vec![b' '; 1024 * 1024 + 1];
    assert!(matches!(
        Priors::validate(&big),
        Err(PriorsError::TooLarge(_))
    ));
    // mean out of range.
    assert!(matches!(
        Priors::validate(&priors_bytes(1, r#"{"mean":1.5,"n":1}"#)),
        Err(PriorsError::BadMean)
    ));
    // mean not finite.
    assert!(matches!(
        Priors::validate(
            br#"{"priors_version":1,"cells":[{"mean":1e999,"n":1}]}"#
                .to_vec()
                .as_slice()
        ),
        Err(PriorsError::Parse) | Err(PriorsError::BadMean)
    ));
    // negative support.
    assert!(matches!(
        Priors::validate(&priors_bytes(1, r#"{"mean":0.5,"n":-1}"#)),
        Err(PriorsError::BadSupport)
    ));
    // over-long string.
    let long = "x".repeat(200);
    assert!(matches!(
        Priors::validate(&priors_bytes(
            1,
            &format!(r#"{{"provider":"{long}","mean":0.5,"n":1}}"#)
        )),
        Err(PriorsError::StringTooLong)
    ));
    // control character in a string.
    assert!(matches!(
        Priors::validate(&priors_bytes(
            1,
            r#"{"provider":"a\u0001b","mean":0.5,"n":1}"#
        )),
        Err(PriorsError::BadControlChar)
    ));
    // too many cells.
    let many = vec![r#"{"mean":0.5,"n":1}"#; c3::router::priors::MAX_CELLS + 1].join(",");
    assert!(matches!(
        Priors::validate(&priors_bytes(1, &many)),
        Err(PriorsError::TooManyCells(_))
    ));
}

#[test]
fn priors_lookup_hierarchy() {
    let bytes = priors_bytes(
        1,
        r#"{"provider":"openai","model":"gpt-6","engine":"codex","purpose":"","topic":"","mean":0.60,"n":10},
           {"provider":"openai","model":"gpt-6","engine":"codex","purpose":"decision","topic":"","mean":0.70,"n":20},
           {"provider":"openai","model":"gpt-6","engine":"codex","purpose":"decision","topic":"api","mean":0.90,"n":30}"#,
    );
    let p = Priors::validate(&bytes).unwrap();
    // topic cell wins.
    let h = p.lookup("openai", "gpt-6", "codex", "decision", &["api".into()]);
    assert!((h.mean - 0.90).abs() < 1e-9);
    // purpose cell.
    let h = p.lookup("openai", "gpt-6", "codex", "decision", &[]);
    assert!((h.mean - 0.70).abs() < 1e-9);
    // lineage cell for an unknown purpose.
    let h = p.lookup("openai", "gpt-6", "codex", "framing", &[]);
    assert!((h.mean - 0.60).abs() < 1e-9);
    // neutral for an unknown lineage.
    let h = p.lookup("other", "x", "codex", "decision", &[]);
    assert!((h.mean - 0.5).abs() < 1e-9);
}

// --------------------------------------------------------------------------- (3) signature

fn test_keypair() -> (SigningKey, String) {
    let secret = [7u8; 32];
    let sk = SigningKey::from_bytes(&secret);
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(sk.verifying_key().to_bytes());
    (sk, pub_b64)
}

fn sig_json(key_id: &str, sk: &SigningKey, file: &[u8]) -> Vec<u8> {
    let sig = sk.sign(file);
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
    format!(r#"{{"alg":"ed25519","key_id":"{key_id}","sig":"{sig_b64}"}}"#).into_bytes()
}

#[test]
fn signature_verifies_and_fails_closed() {
    let _lock = env_lock();
    let (sk, pub_b64) = test_keypair();
    let _env = EnvGuard::set(&[("C3_PRIORS_KEY", Some(&format!("test-key:{pub_b64}")))]);

    let file = br#"{"priors_version":1,"cells":[]}"#.to_vec();
    let good = sig_json("test-key", &sk, &file);
    assert!(sign::verify(&file, &good).is_ok());

    // A flipped file byte fails.
    let mut tampered = file.clone();
    tampered[2] ^= 0x01;
    assert_eq!(sign::verify(&tampered, &good), Err(sign::SigError::Verify));

    // A wrong key id (not trusted) fails.
    let wrong_id = sig_json("nope", &sk, &file);
    assert!(matches!(
        sign::verify(&file, &wrong_id),
        Err(sign::SigError::UnknownKey(_))
    ));

    // A missing/empty .sig fails to parse.
    assert!(matches!(
        sign::verify(&file, b""),
        Err(sign::SigError::Parse)
    ));

    // A different key's signature over the same file fails to verify.
    let other = SigningKey::from_bytes(&[9u8; 32]);
    let other_sig = sig_json("test-key", &other, &file); // claims test-key but signed by another
    assert_eq!(sign::verify(&file, &other_sig), Err(sign::SigError::Verify));

    // F2: an invalid key_id (bad charset) is a parse error and never echoed.
    let bad_id = br#"{"alg":"ed25519","key_id":"has space/../etc","sig":"AAAA"}"#;
    assert_eq!(sign::verify(&file, bad_id), Err(sign::SigError::Parse));
}

#[test]
fn pinned_keys_win_and_user_cannot_shadow_a_pinned_id() {
    // F1: pinned keys are looked up first; a user key with the same id is ignored.
    let _lock = env_lock();
    let file = br#"{"priors_version":1,"cells":[]}"#.to_vec();

    // The maintainer's key is pinned under "hub-key"; an attacker sets C3_PRIORS_KEY to the
    // same id with THEIR key.
    let maintainer = SigningKey::from_bytes(&[5u8; 32]);
    let maintainer_pub =
        base64::engine::general_purpose::STANDARD.encode(maintainer.verifying_key().to_bytes());
    let attacker = SigningKey::from_bytes(&[6u8; 32]);
    let attacker_pub =
        base64::engine::general_purpose::STANDARD.encode(attacker.verifying_key().to_bytes());
    let pinned: &[(&str, &str)] = &[("hub-key", maintainer_pub.as_str())];

    let _env = EnvGuard::set(&[("C3_PRIORS_KEY", Some(&format!("hub-key:{attacker_pub}")))]);

    // Maintainer-signed verifies against the pinned key.
    let good = sig_json("hub-key", &maintainer, &file);
    assert!(sign::verify_with(pinned, &file, &good).is_ok());
    // Attacker-signed under the same id is rejected: the pinned key is used, not the user key.
    let forged = sig_json("hub-key", &attacker, &file);
    assert_eq!(
        sign::verify_with(pinned, &file, &forged),
        Err(sign::SigError::Verify)
    );
}

// --------------------------------------------------------------------------- (5) download / no network

/// A fetcher that records whether it was called and serves canned responses.
struct MockFetcher {
    calls: std::cell::RefCell<Vec<String>>,
    priors: Vec<u8>,
    sig: Vec<u8>,
}
impl Fetcher for MockFetcher {
    fn get(
        &self,
        url: &str,
        _etag: Option<&str>,
        _timeout: std::time::Duration,
    ) -> Result<Fetched, String> {
        self.calls.borrow_mut().push(url.to_string());
        if url.ends_with(".sig") {
            Ok(Fetched {
                status: 200,
                body: self.sig.clone(),
                etag: None,
            })
        } else {
            Ok(Fetched {
                status: 200,
                body: self.priors.clone(),
                etag: Some("\"v1\"".into()),
            })
        }
    }
}

/// A fetcher that panics if it is ever called (to prove a switch skipped the network).
struct NeverFetcher;
impl Fetcher for NeverFetcher {
    fn get(
        &self,
        url: &str,
        _etag: Option<&str>,
        _timeout: std::time::Duration,
    ) -> Result<Fetched, String> {
        panic!("the network was touched: {url}");
    }
}

/// A fetcher that reaches the network but the hub answers `status` with an empty body (404,
/// 3xx, 5xx …), for the cadence/redirect tests.
struct StatusFetcher {
    calls: std::cell::RefCell<usize>,
    status: u16,
}
impl Fetcher for StatusFetcher {
    fn get(
        &self,
        _url: &str,
        _etag: Option<&str>,
        _timeout: std::time::Duration,
    ) -> Result<Fetched, String> {
        *self.calls.borrow_mut() += 1;
        Ok(Fetched {
            status: self.status,
            body: Vec::new(),
            etag: None,
        })
    }
}

#[test]
fn download_switches_and_signature_gate() {
    let _lock = env_lock();
    let dir = temp_dir("dl");
    let now = Utc::now();

    // C3_PRIORS=off — disabled, fetcher never called.
    {
        let _env = EnvGuard::set(&[
            ("C3_PRIORS", Some("off")),
            ("CODEX_CONSULT_TELEMETRY", None),
        ]);
        assert_eq!(
            download::refresh(&dir, now, &NeverFetcher),
            RefreshOutcome::Disabled
        );
    }
    // CODEX_CONSULT_TELEMETRY=off — disabled, fetcher never called.
    {
        let _env = EnvGuard::set(&[
            ("CODEX_CONSULT_TELEMETRY", Some("off")),
            ("C3_PRIORS", None),
        ]);
        assert_eq!(
            download::refresh(&dir, now, &NeverFetcher),
            RefreshOutcome::Disabled
        );
    }
    // An http:// override — refused, fetcher never called.
    {
        let _env = EnvGuard::set(&[
            ("C3_PRIORS", Some("http://xelth.com/T/v2/priors.json")),
            ("CODEX_CONSULT_TELEMETRY", None),
        ]);
        assert_eq!(
            download::refresh(&dir, now, &NeverFetcher),
            RefreshOutcome::Refused
        );
    }

    // A verified update over the (mock) hub, then a valid load.
    {
        let (sk, pub_b64) = test_keypair();
        let priors = priors_bytes(
            1,
            r#"{"provider":"openai","model":"gpt-6","engine":"codex","purpose":"decision","topic":"","mean":0.8,"n":25}"#,
        );
        let sig = sig_json("hub-key", &sk, &priors);
        let mock = MockFetcher {
            calls: std::cell::RefCell::new(Vec::new()),
            priors: priors.clone(),
            sig: sig.clone(),
        };
        let _env = EnvGuard::set(&[
            ("C3_PRIORS", None),
            ("CODEX_CONSULT_TELEMETRY", None),
            ("C3_PRIORS_KEY", Some(&format!("hub-key:{pub_b64}"))),
        ]);
        let out = download::refresh(&dir, now, &mock);
        assert_eq!(
            out,
            RefreshOutcome::Updated,
            "calls: {:?}",
            mock.calls.borrow()
        );
        // The cached copy loads and verifies.
        let (loaded, src) = download::load_cached(&dir, now).expect("cached priors load");
        assert_eq!(src.source, "hub");
        assert_eq!(loaded.cells.len(), 1);

        // A tampered cached signature makes the load fail closed.
        let bad_sig = sig_json("hub-key", &SigningKey::from_bytes(&[1u8; 32]), &priors);
        std::fs::write(dir.join("priors.json.sig"), &bad_sig).unwrap();
        assert!(
            download::load_cached(&dir, now).is_none(),
            "a bad signature discards the copy"
        );
    }
}

#[test]
fn load_cached_ignores_missing_or_mismatched_meta() {
    // F5: a cache with no meta, or one written for a different hub url, is not used.
    let _lock = env_lock();
    let dir = temp_dir("f5");
    let now = Utc::now();
    let (sk, pub_b64) = test_keypair();
    let priors = priors_bytes(1, r#"{"mean":0.5,"n":1}"#);
    let sig = sig_json("hub-key", &sk, &priors);

    // No meta at all → None even if priors.json/sig are present.
    std::fs::write(dir.join("priors.json"), &priors).unwrap();
    std::fs::write(dir.join("priors.json.sig"), &sig).unwrap();
    {
        let _env = EnvGuard::set(&[
            ("C3_PRIORS", None),
            ("CODEX_CONSULT_TELEMETRY", None),
            ("C3_PRIORS_KEY", Some(&format!("hub-key:{pub_b64}"))),
        ]);
        assert!(
            download::load_cached(&dir, now).is_none(),
            "no meta ⇒ not used"
        );
    }
    // Write a verified copy for the default hub, then switch the hub url: the old cache is
    // ignored (a user who changed hubs must not keep the old hub's priors).
    {
        let _env = EnvGuard::set(&[
            ("C3_PRIORS", None),
            ("CODEX_CONSULT_TELEMETRY", None),
            ("C3_PRIORS_KEY", Some(&format!("hub-key:{pub_b64}"))),
        ]);
        let mock = MockFetcher {
            calls: std::cell::RefCell::new(Vec::new()),
            priors: priors.clone(),
            sig: sig.clone(),
        };
        assert_eq!(download::refresh(&dir, now, &mock), RefreshOutcome::Updated);
        assert!(download::load_cached(&dir, now).is_some());
    }
    {
        let _env = EnvGuard::set(&[
            ("C3_PRIORS", Some("https://elsewhere.example/priors.json")),
            ("CODEX_CONSULT_TELEMETRY", None),
            ("C3_PRIORS_KEY", Some(&format!("hub-key:{pub_b64}"))),
        ]);
        assert!(
            download::load_cached(&dir, now).is_none(),
            "mismatched meta.url ⇒ not used"
        );
    }
}

#[test]
fn load_cached_file_mode_refuses_nonfile_and_oversize() {
    // F6: a `file:` source must be a regular file within the size cap.
    let _lock = env_lock();
    let dir = temp_dir("f6");
    let now = Utc::now();

    // A directory path is not a regular file.
    {
        let _env = EnvGuard::set(&[
            ("C3_PRIORS", Some(&format!("file:{}", dir.display()))),
            ("CODEX_CONSULT_TELEMETRY", None),
        ]);
        assert!(
            download::load_cached(&dir, now).is_none(),
            "a directory is not a file"
        );
    }
    // An oversize file (> 1 MiB) is refused before it is read.
    let big = dir.join("big.json");
    std::fs::write(&big, vec![b' '; 1024 * 1024 + 1]).unwrap();
    {
        let _env = EnvGuard::set(&[
            ("C3_PRIORS", Some(&format!("file:{}", big.display()))),
            ("CODEX_CONSULT_TELEMETRY", None),
        ]);
        assert!(
            download::load_cached(&dir, now).is_none(),
            "oversize file refused"
        );
    }
}

#[test]
fn download_refetches_at_most_once_per_day() {
    let _lock = env_lock();
    let dir = temp_dir("cadence");
    let (sk, pub_b64) = test_keypair();
    let priors = priors_bytes(1, r#"{"mean":0.5,"n":1}"#);
    let sig = sig_json("hub-key", &sk, &priors);
    let _env = EnvGuard::set(&[
        ("C3_PRIORS", None),
        ("CODEX_CONSULT_TELEMETRY", None),
        ("C3_PRIORS_KEY", Some(&format!("hub-key:{pub_b64}"))),
    ]);
    let now = Utc::now();
    let mock = MockFetcher {
        calls: std::cell::RefCell::new(Vec::new()),
        priors,
        sig,
    };
    assert_eq!(download::refresh(&dir, now, &mock), RefreshOutcome::Updated);
    // A second refresh within 24 h is skipped (no fetch).
    assert_eq!(
        download::refresh(&dir, now, &NeverFetcher),
        RefreshOutcome::Skipped
    );
    // After 24 h it fetches again.
    let later = now + chrono::Duration::hours(25);
    assert_eq!(
        download::refresh(&dir, later, &mock),
        RefreshOutcome::Updated
    );
}

#[test]
fn a_404_or_failure_gates_the_next_fetch_for_24h() {
    // F4: the cadence counts ATTEMPTS, so a dead hub is not polled every consultation.
    let _lock = env_lock();
    let _env = EnvGuard::set(&[("C3_PRIORS", None), ("CODEX_CONSULT_TELEMETRY", None)]);
    let now = Utc::now();

    // 404: the normal "no priors" answer today.
    let dir = temp_dir("cad404");
    let f404 = StatusFetcher {
        calls: std::cell::RefCell::new(0),
        status: 404,
    };
    assert_eq!(
        download::refresh(&dir, now, &f404),
        RefreshOutcome::NoPriors
    );
    assert_eq!(*f404.calls.borrow(), 1);
    // Within 24 h: skipped, the fetcher is NOT called.
    assert_eq!(
        download::refresh(&dir, now, &NeverFetcher),
        RefreshOutcome::Skipped
    );
    // After 24 h: it reaches the hub again.
    let later = now + chrono::Duration::hours(25);
    assert_eq!(
        download::refresh(&dir, later, &f404),
        RefreshOutcome::NoPriors
    );
    assert_eq!(*f404.calls.borrow(), 2);

    // A 3xx is refused as a redirect (F3), and equally gates the next 24 h.
    let dir = temp_dir("cad3xx");
    let f302 = StatusFetcher {
        calls: std::cell::RefCell::new(0),
        status: 302,
    };
    assert_eq!(
        download::refresh(&dir, now, &f302),
        RefreshOutcome::Failed("hub answered with a redirect".to_string())
    );
    assert_eq!(
        download::refresh(&dir, now, &NeverFetcher),
        RefreshOutcome::Skipped
    );
}

// --------------------------------------------------------------------------- (4) replay

fn fixture_sessions() -> SessionsFile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("router")
        .join("sessions.json");
    let bytes = std::fs::read(path).expect("router fixture");
    SessionsFile::read(&bytes).expect("fixture parses")
}

#[test]
fn replay_of_the_real_fixture_is_identical() {
    let file = fixture_sessions();
    let report = c3::router::replay::replay_sessions(&file, None);
    assert!(
        report.panels >= 1,
        "the fixture has at least one routed panel"
    );
    assert!(
        report.identical(),
        "replay differs: {:?}",
        report.first_diff
    );
}

#[test]
fn replay_detects_a_tampered_picked() {
    let mut file = fixture_sessions();
    // Flip a seat's position in the first routed panel.
    for e in file.codex.consults.iter_mut() {
        if let Some(p) = e.panel.as_mut() {
            if let Some(r) = p.routing.as_mut() {
                if let Some(first) = r.picked.first_mut() {
                    first.position += 100;
                    break;
                }
            }
        }
    }
    let report = c3::router::replay::replay_sessions(&file, None);
    assert!(!report.identical(), "a tampered picked must be caught");
    assert!(report.first_diff.is_some());
}
