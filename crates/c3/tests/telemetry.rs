//! Milestone 5 privacy and spool tests for the T-hub telemetry client.
//!
//! Nothing here reaches xelth.com: every network test points the client at a local
//! [`TcpListener`] mock. The mock answers the T-hub event/complaint/delete shapes.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::Duration;

use c3_core::ledger::{FindingCounts, FormatRetry, LedgerEntry, Reviewer, Usage};

use c3::telemetry::{self, Config, Event, RatingEvent, RatingInput, Spool};

// --------------------------------------------------------------------------- helpers

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A fresh empty temporary directory unique to this test run.
fn temp_dir(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("c3-tel-{tag}-{pid}-{n}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Spin a one-shot HTTP mock. It answers `code`/`body` to every request and forwards each
/// request body over the returned channel. Returns the `.../T` base URL.
fn spawn_mock(code: u16, reason: &'static str, body: &'static str) -> (String, Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = channel();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut content_length = 0usize;
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_length = v.trim().parse().unwrap_or(0);
                }
            }
            let mut buf = vec![0u8; content_length];
            let _ = reader.read_exact(&mut buf);
            let _ = tx.send(String::from_utf8_lossy(&buf).to_string());
            let resp = format!(
                "HTTP/1.1 {code} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    (format!("http://{addr}/T"), rx)
}

/// A ledger entry whose every text field is a secret/path, to prove none can leak.
fn poisoned_entry() -> LedgerEntry {
    const POISON: &str = "LEAK /home/u/.ssh/id_rsa secret=sk-live-DEADBEEF password token";
    LedgerEntry {
        n: 7,
        when: POISON.into(),
        purpose: POISON.into(),
        consult_id: POISON.into(),
        reviewer: Reviewer {
            provider: POISON.into(),
            provider_source: POISON.into(),
            model: POISON.into(),
            model_source: POISON.into(),
            engine: POISON.into(),
            harness: POISON.into(),
            provider_fingerprint: POISON.into(),
            identity_note: POISON.into(),
            ..Default::default()
        },
        lineage: POISON.into(),
        preflight: POISON.into(),
        preflight_warning: POISON.into(),
        parent_thread: POISON.into(),
        thread: POISON.into(),
        thread_source: POISON.into(),
        thread_candidate: POISON.into(),
        mode: POISON.into(),
        command: POISON.into(),
        brief: POISON.into(),
        model: POISON.into(),
        bridge_outcome: POISON.into(),
        verdict: POISON.into(),
        verdict_reason: POISON.into(),
        fingerprint_note: POISON.into(),
        finished_at: POISON.into(),
        structured: true,
        wall_seconds: 41.5,
        usage: Some(Usage {
            input_tokens: 12000,
            output_tokens: 900,
            ..Default::default()
        }),
        findings: FindingCounts {
            blocker: 1,
            major: 2,
            minor: 0,
            note: 1,
            ..Default::default()
        },
        format_retry: Some(FormatRetry {
            attempted: true,
            ..Default::default()
        }),
        ..Default::default()
    }
}

// --------------------------------------------------------------------------- privacy

#[test]
fn allowlist_never_leaks_secrets_or_paths() {
    let entry = poisoned_entry();
    let event = Event::from_ledger(&entry, Some(3), "testinstance");
    let serialized = serde_json::to_string(&event).unwrap();

    for needle in [
        "LEAK", "id_rsa", "sk-live", "DEADBEEF", "/home", "password", ".ssh", "secret=",
    ] {
        assert!(
            !serialized.contains(needle),
            "the payload leaked `{needle}`: {serialized}"
        );
    }

    let v: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    let d = &v["details"];
    // (wave 2c, F02-2) classes only: an unknown engine, no vendor, a purpose and an outcome
    // outside their closed sets
    assert_eq!(d["engine"], "other");
    assert_eq!(d["provider"], "other");
    assert_eq!(d["model"], "other");
    assert_eq!(d["purpose"], "other");
    assert_eq!(d["outcome"], "failed:bridge");
    assert_eq!(v["severity"], "error");
    assert_eq!(v["tags"], serde_json::json!(["other", "other"]));
    // The numeric/boolean allowlist still carries the useful shape.
    assert_eq!(d["tokens_in"], 12000);
    assert_eq!(d["tokens_out"], 900);
    assert_eq!(d["findings"], 4);
    assert_eq!(d["structured"], true);
    assert_eq!(d["format_retry"], true);
    assert_eq!(d["panel_size"], 3);
    // The title mirrors the purpose class, never the raw text.
    assert_eq!(v["title"], "other");
}

/// (wave 2c, F02-2) A private but syntactically harmless label - `customer-acme` as the roster
/// provider, the model, the purpose, the failure class and the verdict, on a private endpoint -
/// never leaves the machine: not in the consultation event, not in the rating event (whose
/// reviewer goes through the SAME classifier), not in a complaint's last-run summary.
#[test]
fn a_private_label_never_reaches_any_payload() {
    let entry = LedgerEntry {
        n: 4,
        when: "2026-10-08T10:00:00+02:00".into(),
        purpose: "customer-acme".into(),
        consult_ref: Some("6f1c2a9e-4b7d-4e2a-9c3f-0d8e5b7a1c24".into()),
        reviewer: Reviewer {
            provider: "customer-acme".into(),
            model: "customer-acme-7b".into(),
            engine: "codex".into(),
            provider_config: serde_json::json!({"base_url": "https://llm.customer-acme.example/v1"}),
            ..Default::default()
        },
        bridge_outcome: "failed: customer-acme".into(),
        provider_failure: Some(c3_core::ledger::ProviderFailure {
            class: "customer-acme".into(),
            ..Default::default()
        }),
        verdict: "customer-acme".into(),
        ..Default::default()
    };
    let ev = serde_json::to_string(&Event::from_ledger(&entry, Some(1), "i")).unwrap();
    assert!(!ev.contains("acme"), "{ev}");
    let v: serde_json::Value = serde_json::from_str(&ev).unwrap();
    assert_eq!(v["details"]["provider"], "other");
    assert_eq!(v["details"]["model"], "other");
    assert_eq!(v["details"]["outcome"], "failed:unknown");
    let judge = telemetry::Judge::unknown();
    let input = RatingInput {
        entry: &entry,
        mark: "no",
        rated_at: chrono::DateTime::parse_from_rfc3339("2026-10-08T12:00:00+02:00").unwrap(),
        consult_when: None,
        judge: &judge,
        rating_rev: Some(1),
    };
    let rating = serde_json::to_string(&RatingEvent::from_rating(&input, "i")).unwrap();
    assert!(!rating.contains("acme"), "{rating}");
    // the same reviewer classes in both events (one code path)
    let r: serde_json::Value = serde_json::from_str(&rating).unwrap();
    for k in ["engine", "provider", "model", "purpose"] {
        assert_eq!(r["details"][k], v["details"][k], "{k}");
    }
    // a known vendor on the same code path: the class and the closed-list model
    let mut known = entry.clone();
    known.reviewer.provider_config = serde_json::json!({"base_url": "https://api.z.ai/api/v1"});
    known.reviewer.model = "GLM-5.3".into();
    let k = serde_json::to_value(Event::from_ledger(&known, Some(1), "i")).unwrap();
    assert_eq!(k["details"]["provider"], "zai");
    assert_eq!(k["details"]["model"], "glm-5.3");
    assert_eq!(k["tags"], serde_json::json!(["zai", "glm-5.3"]));
}

#[test]
fn rating_event_allowlist_never_leaks_secrets_or_paths() {
    // The rating event (0.6.1 shape) of a poisoned entry: every text input is a secret/path, the
    // judge is unknown, and none of it reaches the payload.
    let entry = poisoned_entry();
    let judge = telemetry::Judge::unknown();
    let at = chrono::DateTime::parse_from_rfc3339("2026-10-08T21:00:00+02:00").unwrap();
    let input = RatingInput {
        entry: &entry,
        mark: " Yes ",
        rated_at: at,
        consult_when: Some("LEAK /home/u/.ssh/id_rsa"),
        judge: &judge,
        rating_rev: Some(2),
    };
    let event = RatingEvent::from_rating(&input, "testinstance");
    let serialized = serde_json::to_string(&event).unwrap();

    for needle in [
        "LEAK", "id_rsa", "sk-live", "DEADBEEF", "/home", "password", ".ssh", "secret=",
    ] {
        assert!(
            !serialized.contains(needle),
            "the rating payload leaked `{needle}`: {serialized}"
        );
    }
    let v: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    assert_eq!(v["event_type"], "rating");
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
    assert_eq!(
        keys,
        [
            "app_id",
            "app_version",
            "instance_id",
            "event_type",
            "severity",
            "title",
            "details",
            "tags",
            "client_time",
            "os",
            "runtime"
        ]
    );
    let d = &v["details"];
    let dkeys: Vec<&str> = d.as_object().unwrap().keys().map(|k| k.as_str()).collect();
    // the plugin's exact order; no consult_ref (the entry has none)
    assert_eq!(
        dkeys,
        [
            "engine",
            "provider",
            "model",
            "purpose",
            "mark",
            "age_days",
            "bridge_version",
            "os",
            "ps_version",
            "judge",
            "rating_rev"
        ]
    );
    assert_eq!(d["engine"], "other");
    assert_eq!(d["mark"], "yes");
    assert_eq!(d["age_days"], 0, "no consultation time parses: 0");
    assert_eq!(
        d["judge"],
        serde_json::json!({"provider": "other", "model": "other", "source": "unknown"})
    );
    assert_eq!(d["rating_rev"], 2);
    assert_eq!(d["ps_version"], "unknown");
    // the title is the mark, the client time the mark's own `when` in UTC
    assert_eq!(v["title"], "yes");
    assert_eq!(v["severity"], "info");
    assert_eq!(v["client_time"], "2026-10-08T19:00:00Z");
    assert_eq!(
        v["tags"],
        serde_json::json!([d["provider"].clone(), d["model"].clone()])
    );
}

#[test]
fn example_event_is_printed_for_the_report() {
    let entry = LedgerEntry {
        n: 12,
        purpose: "diff-review".into(),
        reviewer: Reviewer {
            provider: "openai".into(),
            model: "gpt-5.1".into(),
            engine: "codex".into(),
            provider_config: serde_json::json!({"builtin": "openai"}),
            ..Default::default()
        },
        bridge_outcome: "usable reply".into(),
        structured: true,
        wall_seconds: 41.2,
        usage: Some(Usage {
            input_tokens: 12000,
            output_tokens: 900,
            ..Default::default()
        }),
        findings: FindingCounts {
            major: 4,
            ..Default::default()
        },
        ..Default::default()
    };
    let event = Event::from_ledger(
        &entry,
        Some(1),
        "3f1c00000000000000000000000000000000000000000000000000000000007c",
    );
    let pretty = serde_json::to_string_pretty(&event).unwrap();
    println!("EXAMPLE EVENT >>>\n{pretty}\n<<< EXAMPLE EVENT");
    let v: serde_json::Value = serde_json::from_str(&pretty).unwrap();
    assert_eq!(v["details"]["outcome"], "usable");
    assert_eq!(v["details"]["engine"], "codex");
    assert_eq!(v["details"]["provider"], "openai");
    assert_eq!(v["details"]["model"], "gpt-5.1");
    assert_eq!(v["severity"], "info");
}

// --------------------------------------------------------------------------- instance id

#[test]
fn instance_id_is_stable_and_salted() {
    let dir = temp_dir("iid");
    let a = telemetry::instance_id_in(&dir);
    let b = telemetry::instance_id_in(&dir);
    assert_eq!(a, b, "same directory yields the same id across runs");
    assert_eq!(a.len(), 64, "sha256 hex");
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(dir.join("salt").is_file(), "the salt is written once");

    let other = temp_dir("iid2");
    let c = telemetry::instance_id_in(&other);
    assert_ne!(a, c, "a different salt yields a different id");
}

#[test]
fn instance_id_if_exists_is_read_only_until_created() {
    let dir = temp_dir("iid-ro");
    assert_eq!(
        telemetry::instance_id_if_exists_in(&dir),
        None,
        "no salt yet"
    );
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        0,
        "status must not create the salt file (or anything else) on first use"
    );

    let created = telemetry::instance_id_in(&dir);
    assert_eq!(
        telemetry::instance_id_if_exists_in(&dir),
        Some(created),
        "once the salt exists, it reports the same id without rewriting it"
    );
}

// --------------------------------------------------------------------------- spool

/// An instance id of the client's shape (64 lower-case hex digits): the sender closes every queued
/// event and discards one without such an id (F09-5).
const TEST_INSTANCE: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn enqueue_one(spool: &Spool, purpose: &str) {
    let entry = LedgerEntry {
        purpose: purpose.into(),
        reviewer: Reviewer {
            engine: "codex".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let event = Event::from_ledger(&entry, Some(1), TEST_INSTANCE);
    spool.enqueue(&event).unwrap();
}

#[test]
fn spool_flush_sends_and_empties_on_success() {
    let dir = temp_dir("flush-ok");
    let (hub, rx) = spawn_mock(
        200,
        "OK",
        r#"{"ok":true,"accepted":2,"event_ids":[],"scrubbed":0}"#,
    );
    let spool = Spool::new(&dir, hub);
    enqueue_one(&spool, "framing");
    enqueue_one(&spool, "decision");
    assert_eq!(spool.pending(), 2);

    let report = spool.flush().unwrap();
    assert_eq!(report.sent, 2);
    assert!(report.attempted);
    assert_eq!(spool.pending(), 0, "accepted events are removed");

    let received = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(received.contains("\"events\""));
    assert!(received.contains("consultation"));
    assert!(
        !received.contains("scrubbed"),
        "the client never sends server-only keys"
    );
}

#[test]
fn spool_flush_403_keeps_the_spool() {
    let dir = temp_dir("flush-403");
    let (hub, _rx) = spawn_mock(403, "Forbidden", r#"{"ok":false}"#);
    let spool = Spool::new(&dir, hub);
    enqueue_one(&spool, "framing");
    assert_eq!(spool.pending(), 1);

    let report = spool.flush().unwrap();
    assert_eq!(report.sent, 0);
    assert!(report.attempted);
    assert_eq!(spool.pending(), 1, "a 403 keeps the spool for next run");
}

#[test]
fn spool_drops_events_older_than_seven_days() {
    let dir = temp_dir("drop");
    // Write a stale line directly (client_time eight days ago); no server needed.
    let stale = r#"{"app_id":"c3","event_type":"consultation","client_time":"2000-01-01T00:00:00Z","details":{}}"#;
    std::fs::write(dir.join("spool.ndjson"), format!("{stale}\n")).unwrap();
    let spool = Spool::new(&dir, "http://127.0.0.1:1/T"); // unreachable, must not be used
    let report = spool.flush().unwrap();
    assert_eq!(report.dropped_stale, 1);
    assert!(!report.attempted, "an all-stale spool sends nothing");
    assert_eq!(spool.pending(), 0, "stale events are dropped");
}

// --------------------------------------------------------------------------- complaints

#[test]
fn complaint_previews_exact_payload_and_sends() {
    let dir = temp_dir("complain");
    let (hub, rx) = spawn_mock(
        200,
        "OK",
        r#"{"ok":true,"complaint_id":1,"public_ref":"T-7KQ4-M2XZ"}"#,
    );
    let seen = std::cell::RefCell::new(String::new());
    let confirm = |payload: &str| {
        *seen.borrow_mut() = payload.to_string();
        true
    };
    let reference = telemetry::complain_to(
        &hub,
        &dir,
        "the panel hung on peak windows",
        Some("consult n=3 purpose=diff-review engine=codex verdict=usable findings=2 wall=41s"),
        confirm,
    )
    .unwrap();

    assert_eq!(reference.as_deref(), Some("T-7KQ4-M2XZ"));
    let preview = seen.borrow();
    assert!(preview.contains("the panel hung on peak windows"));
    assert!(preview.contains("last_run"));
    assert!(preview.contains("purpose=diff-review"));

    let received = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(received.contains("\"text\""));

    // The public reference is stored for a later forget-me.
    let refs = std::fs::read_to_string(dir.join("refs.ndjson")).unwrap();
    assert!(refs.contains("T-7KQ4-M2XZ"));
}

#[test]
fn complaint_declined_is_not_sent() {
    let dir = temp_dir("complain-no");
    let (hub, _rx) = spawn_mock(200, "OK", r#"{"public_ref":"T-0000-0000"}"#);
    let reference = telemetry::complain_to(&hub, &dir, "text", None, |_payload| false).unwrap();
    assert_eq!(reference, None, "a declined complaint sends nothing");
    assert!(!dir.join("refs.ndjson").exists());
}

/// A scripted HTTP mock: answers `(code, body)` in order (the last one repeats) and records every
/// request `(method, path?query)`. Returns the `.../T` base and the request log.
fn scripted_mock(
    answers: Vec<(u16, &'static str)>,
) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log2 = log.clone();
    thread::spawn(move || {
        for (k, stream) in listener.incoming().enumerate() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            let _ = reader.read_line(&mut first);
            let mut content_length = 0usize;
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_length = v.trim().parse().unwrap_or(0);
                }
            }
            let mut buf = vec![0u8; content_length];
            let _ = reader.read_exact(&mut buf);
            let mut parts = first.split_whitespace();
            log2.lock().unwrap().push(format!(
                "{} {}",
                parts.next().unwrap_or(""),
                parts.next().unwrap_or("")
            ));
            let (code, body) = answers[k.min(answers.len() - 1)];
            let resp = format!(
                "HTTP/1.1 {code} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    (format!("http://{addr}/T"), log)
}

fn hub_at(base: &str) -> telemetry::Hub {
    telemetry::Hub {
        base: base.to_string(),
        source: "test".into(),
        error: String::new(),
    }
}

/// (F02-3) A failed DELETE keeps the identity and the proof: nothing local is removed, the pending
/// deletion records the instance id and the reference, producers spool nothing and the sender
/// sends nothing meanwhile; the retry - even after the salt is gone - names the SAME instance and
/// reference, and only its confirmation removes the local files.
#[test]
fn forget_failed_delete_keeps_the_identity_and_the_retry_uses_it() {
    let dir = temp_dir("forget-retry");
    let iid = telemetry::instance_id_in(&dir);
    let spool = Spool::new(&dir, "http://127.0.0.1:9/T");
    enqueue_one(&spool, "framing");
    std::fs::write(dir.join("refs.ndjson"), "{\"public_ref\":\"CC-7Q\"}\n").unwrap();
    let (hub, log) = scripted_mock(vec![
        (503, r#"{"ok":false,"error":"maintenance"}"#),
        (200, r#"{"ok":true,"deleted":3}"#),
    ]);
    let req = telemetry::ForgetRequest {
        public_ref: Some("CC-7Q".into()),
        local: true,
        yes: true,
    };
    let first = telemetry::forget_at(&hub_at(&hub), &dir, &req, |_| true);
    assert_eq!(first.exit, 3, "{:?}", first.lines);
    assert!(!first.server_deleted);
    let text = first.lines.join("\n");
    assert!(
        text.contains("did not confirm the deletion (HTTP 503: maintenance)"),
        "{text}"
    );
    assert!(
        text.contains("NOTHING was deleted - not there and not here"),
        "{text}"
    );
    assert!(!text.contains("drops it on the next successful DELETE"));
    // nothing local was removed
    assert!(dir.join("salt").is_file() && dir.join("refs.ndjson").is_file());
    assert_eq!(spool.pending(), 1);
    let pending = telemetry::pending_deletion_in(&dir).expect("pending deletion recorded");
    assert_eq!(pending.instance_id, iid);
    assert_eq!(pending.public_ref, "CC-7Q");
    assert_eq!(pending.attempts, 1);
    // meanwhile: a producer spools nothing, the sender sends nothing
    let entry = LedgerEntry {
        purpose: "framing".into(),
        ..Default::default()
    };
    let ev = Event::from_ledger(&entry, Some(1), "x");
    assert!(spool.enqueue(&ev).is_err());
    assert_eq!(spool.pending(), 1);
    let r = Spool::new(&dir, hub.clone()).flush().unwrap();
    assert!(!r.attempted && r.skipped.contains("deletion"), "{r:?}");
    // the salt is gone (a local-only deletion, a crash): the pending record still identifies it
    std::fs::remove_file(dir.join("salt")).unwrap();
    let retry = telemetry::forget_at(
        &hub_at(&hub),
        &dir,
        &telemetry::ForgetRequest {
            public_ref: None,
            local: true,
            yes: true,
        },
        |_| true,
    );
    assert_eq!(retry.exit, 0, "{:?}", retry.lines);
    assert!(retry.server_deleted);
    let reqs = log.lock().unwrap().clone();
    let want = format!("DELETE /T/v2/instances/{iid}?public_ref=CC-7Q");
    assert_eq!(
        reqs,
        [want.clone(), want],
        "both DELETEs name the same instance and proof"
    );
    // confirmed: now the local files go, and the pending record with them
    assert!(!dir.join("refs.ndjson").exists() && !dir.join("spool.ndjson").exists());
    assert!(telemetry::pending_deletion_in(&dir).is_none());
    assert!(retry.lines.join("\n").contains("removed locally - "));
}

/// The plugin's -Forget refusals and -Local alone (asks; the intake keeps the data).
#[test]
fn forget_refusals_and_local_alone() {
    let dir = temp_dir("forget-local");
    let (hub, log) = scripted_mock(vec![(200, r#"{"ok":true}"#)]);
    let none = telemetry::forget_at(
        &hub_at(&hub),
        &dir,
        &telemetry::ForgetRequest::default(),
        |_| true,
    );
    assert_eq!(none.exit, 1);
    assert!(none.lines.join("\n").contains("needs -PublicRef <ref>"));
    // a reference without a salt: no instance id, no request
    let nosalt = telemetry::forget_at(
        &hub_at(&hub),
        &dir,
        &telemetry::ForgetRequest {
            public_ref: Some("CC-7Q".into()),
            ..Default::default()
        },
        |_| true,
    );
    assert_eq!(nosalt.exit, 1);
    assert!(nosalt.lines.join("\n").contains("no instance id"));
    assert!(log.lock().unwrap().is_empty());
    // -Local alone: says the intake keeps what was sent, asks; no -> nothing removed
    let iid = telemetry::instance_id_in(&dir);
    let no = telemetry::forget_at(
        &hub_at(&hub),
        &dir,
        &telemetry::ForgetRequest {
            local: true,
            ..Default::default()
        },
        |_| false,
    );
    assert_eq!(no.exit, 1);
    let t = no.lines.join("\n");
    assert!(
        t.contains(&format!(
            "the intake still holds what this machine sent (instance {iid})"
        )),
        "{t}"
    );
    assert!(t.contains("-Forget -PublicRef <ref> BEFORE -Local"));
    assert!(t.contains("nothing removed (no confirmation"));
    assert!(dir.join("salt").is_file());
    let yes = telemetry::forget_at(
        &hub_at(&hub),
        &dir,
        &telemetry::ForgetRequest {
            local: true,
            ..Default::default()
        },
        |_| true,
    );
    assert_eq!(yes.exit, 0);
    assert!(!dir.join("salt").exists());
    assert!(log.lock().unwrap().is_empty(), "-Local alone sends nothing");
}

// --------------------------------------------------------------------------- the outbox (F02-1)

/// (F02-1) The lost-update race: the sender has read event A and holds its POST; event B is
/// appended meanwhile; the intake accepts A; A is removed and B STAYS queued.
#[test]
fn outbox_keeps_an_event_appended_while_the_sender_posts() {
    let dir = temp_dir("outbox-race");
    let spool = Spool::new(&dir, "http://unused.invalid/T");
    enqueue_one(&spool, "framing");
    let (to_test, posted) = std::sync::mpsc::channel::<String>();
    let (to_sender, proceed) = std::sync::mpsc::channel::<()>();
    let d2 = dir.clone();
    let sender = thread::spawn(move || {
        Spool::new(&d2, "http://unused.invalid/T")
            .flush_with(|body| {
                to_test.send(body.to_string()).unwrap();
                proceed.recv().unwrap(); // the intake holds its answer
                true
            })
            .unwrap()
    });
    let body_a = posted.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(body_a.contains("\"framing\""));
    // B arrives while A's POST is in flight
    enqueue_one(&spool, "decision");
    to_sender.send(()).unwrap();
    let report = sender.join().unwrap();
    assert_eq!(report.sent, 1);
    let left = std::fs::read_to_string(dir.join("spool.ndjson")).unwrap();
    assert_eq!(spool.pending(), 1, "B is still queued: {left}");
    assert!(
        left.contains("decision") && !left.contains("framing"),
        "{left}"
    );
}

/// One sender at a time: a flush that finds the sender lock held skips (nothing posted twice).
#[test]
fn outbox_second_sender_skips_while_the_first_holds_the_lock() {
    let dir = temp_dir("outbox-two");
    let spool = Spool::new(&dir, "http://unused.invalid/T");
    enqueue_one(&spool, "framing");
    let (to_test, posted) = std::sync::mpsc::channel::<()>();
    let (to_sender, proceed) = std::sync::mpsc::channel::<()>();
    let d2 = dir.clone();
    let first = thread::spawn(move || {
        Spool::new(&d2, "http://unused.invalid/T")
            .flush_with(|_| {
                to_test.send(()).unwrap();
                proceed.recv().unwrap();
                true
            })
            .unwrap()
    });
    posted.recv_timeout(Duration::from_secs(10)).unwrap();
    let second = spool
        .flush_with(|_| panic!("a second sender must not post"))
        .unwrap();
    assert!(second.skipped.contains("another flush"), "{second:?}");
    to_sender.send(()).unwrap();
    assert_eq!(first.join().unwrap().sent, 1);
    assert_eq!(spool.pending(), 0);
}

/// The spool line is the plugin's `{v, kind, queued_unix, body}`; a rating backfilled long after
/// its mark (an old client_time) is NOT dropped - the 7 days run from the queue time; a line C3
/// wrote before wave 2 (the raw event) is still delivered; a torn line is ended by the next append
/// and dropped by the sender, the new line stays whole; a send that fails keeps everything.
#[test]
fn outbox_line_shape_legacy_lines_torn_lines_and_queue_time() {
    let dir = temp_dir("outbox-shape");
    let spool = Spool::new(&dir, "http://unused.invalid/T");
    let old = serde_json::json!({"app_id": "c3", "instance_id": TEST_INSTANCE, "event_type": "rating", "client_time": "2000-01-01T00:00:00Z", "details": {}});
    spool.enqueue_line(&old).unwrap();
    let text = std::fs::read_to_string(dir.join("spool.ndjson")).unwrap();
    let line: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    let keys: Vec<&str> = line
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    assert_eq!(keys, ["v", "kind", "queued_unix", "body"]);
    assert_eq!(line["kind"], "event");
    assert!(line["body"].is_string());
    // a legacy raw line (fresh client_time) and a torn line without its newline
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let legacy = format!(
        r#"{{"app_id":"c3","instance_id":"{TEST_INSTANCE}","event_type":"consultation","client_time":"{now}","details":{{}}}}"#
    );
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(dir.join("spool.ndjson"))
        .unwrap();
    write!(f, "{legacy}\n{{\"v\":1,\"kind\":\"ev").unwrap();
    drop(f);
    enqueue_one(&spool, "decision");
    assert_eq!(spool.pending(), 4);
    // a failed send keeps everything but the torn line (dropped as unreadable)
    let kept = spool.flush_with(|_| false).unwrap();
    assert!(kept.attempted && kept.sent == 0);
    assert_eq!(kept.dropped_stale, 1);
    assert_eq!(spool.pending(), 3);
    let posted = std::sync::Mutex::new(String::new());
    let sent = spool
        .flush_with(|b| {
            *posted.lock().unwrap() = b.to_string();
            true
        })
        .unwrap();
    assert_eq!(
        sent.sent, 3,
        "the old rating (queued now), the legacy line and the new event"
    );
    assert_eq!(spool.pending(), 0);
    let body = posted.lock().unwrap().clone();
    assert!(body.contains("2000-01-01T00:00:00Z") && body.contains("decision"));
}

// --------------------------------------------------------------------------- wave 2d (F09-2..F09-5)

fn forget_req(public_ref: Option<&str>, local: bool) -> telemetry::ForgetRequest {
    telemetry::ForgetRequest {
        public_ref: public_ref.map(str::to_string),
        local,
        yes: true,
    }
}

fn consultation_of(instance: &str, purpose: &str) -> Event {
    let entry = LedgerEntry {
        purpose: purpose.into(),
        reviewer: Reviewer {
            engine: "codex".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    Event::from_ledger(&entry, Some(1), instance)
}

/// (F09-2 / RC2, schedule 1) A sender passes its early deletion check and waits at a barrier
/// BEFORE the sender lock; meanwhile a forget (with `--local` and without) takes the locks, the
/// intake does NOT confirm the DELETE, the pending deletion is recorded and the locks released;
/// the sender resumes and decides again under the sender lock and the spool lock: it skips, the
/// injected sender is NEVER called, and the queued event stays.
#[test]
fn a_forget_failing_while_a_flush_waits_never_lets_the_flush_post_rc2() {
    for local in [true, false] {
        let dir = temp_dir(if local {
            "rc2-barrier-local"
        } else {
            "rc2-barrier"
        });
        let iid = telemetry::instance_id_in(&dir);
        let spool = Spool::new(&dir, "http://unused.invalid/T");
        spool.enqueue(&consultation_of(&iid, "framing")).unwrap();
        let (hub, log) = scripted_mock(vec![(503, r#"{"ok":false,"error":"maintenance"}"#)]);
        let (at_barrier, checked) = channel::<()>();
        let (release, go) = channel::<()>();
        let posted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (d2, p2) = (dir.clone(), posted.clone());
        let sender = thread::spawn(move || {
            let after = || {
                at_barrier.send(()).unwrap();
                go.recv().unwrap();
            };
            Spool::new(&d2, "http://unused.invalid/T")
                .flush_hooked(
                    |_| {
                        p2.store(true, Ordering::SeqCst);
                        true
                    },
                    &telemetry::FlushHooks {
                        after_check: Some(&after),
                        ..Default::default()
                    },
                )
                .unwrap()
        });
        checked.recv_timeout(Duration::from_secs(10)).unwrap();
        let out = telemetry::forget_at(
            &hub_at(&hub),
            &dir,
            &forget_req(Some("CC-7Q"), local),
            |_| true,
        );
        assert_eq!(out.exit, 3, "{:?}", out.lines);
        let txn = telemetry::pending_deletion_in(&dir).expect("the pending deletion");
        assert_eq!(txn.phase, telemetry::PHASE_PENDING);
        assert_eq!(txn.instance_id, iid);
        release.send(()).unwrap();
        let report = sender.join().unwrap();
        assert!(
            !posted.load(Ordering::SeqCst),
            "the sender posted while a deletion was pending"
        );
        assert!(
            !report.attempted && report.skipped.contains("deletion"),
            "{report:?}"
        );
        assert_eq!(spool.pending(), 1);
        assert_eq!(
            log.lock().unwrap().clone(),
            [format!("DELETE /T/v2/instances/{iid}?public_ref=CC-7Q")]
        );
    }
}

/// (F09-3 / RC2, schedule 2) The intake CONFIRMS the DELETE and the local cleanup is interrupted
/// (an injected removal failure where the spool goes - the process dies there): the transaction
/// stays, `cleaning`, with the OLD instance id, and the old instance's queued event with it; the
/// salt is still there (queued data goes before identity material). Then the state of the review -
/// the salt gone, the spool preserved - and a restart: a producer spools nothing, the sender posts
/// NOTHING of the old instance and finishes the cleanup with the retained identity, the record
/// removed last; only then does a NEW instance's event go out.
#[test]
fn a_confirmed_delete_with_an_interrupted_cleanup_resumes_and_never_posts_the_old_instance_rc2() {
    let dir = temp_dir("rc2-resume");
    let old_iid = telemetry::instance_id_in(&dir);
    let spool = Spool::new(&dir, "http://unused.invalid/T");
    spool
        .enqueue(&consultation_of(&old_iid, "framing"))
        .unwrap();
    std::fs::write(dir.join("refs.ndjson"), "{\"public_ref\":\"CC-7Q\"}\n").unwrap();
    let (hub, log) = scripted_mock(vec![(200, r#"{"ok":true,"deleted":3}"#)]);
    let spool_path = dir.join("spool.ndjson");
    let phases = std::sync::Mutex::new(Vec::<String>::new());
    let interrupted = telemetry::forget_with(
        &hub_at(&hub),
        &dir,
        &forget_req(Some("CC-7Q"), true),
        |_| true,
        &|p: &std::path::Path| {
            phases.lock().unwrap().push(
                telemetry::pending_deletion_in(&dir)
                    .map(|t| t.phase)
                    .unwrap_or_default(),
            );
            if p == spool_path {
                Err(std::io::Error::other("injected: the process died here"))
            } else {
                std::fs::remove_file(p)
            }
        },
    );
    assert!(interrupted.server_deleted);
    assert_eq!(interrupted.exit, 1, "{:?}", interrupted.lines);
    let t = interrupted.lines.join("\n");
    assert!(t.contains("the local deletion did not finish"), "{t}");
    assert!(t.contains(&format!("keeps instance {old_iid}")), "{t}");
    assert_eq!(phases.lock().unwrap().clone(), ["cleaning"]);
    let txn = telemetry::pending_deletion_in(&dir).expect("the transaction survives");
    assert_eq!(txn.phase, telemetry::PHASE_CLEANING);
    assert_eq!(txn.instance_id, old_iid);
    assert_eq!(
        spool.pending(),
        1,
        "the old instance's event is still queued"
    );
    assert!(
        dir.join("salt").is_file(),
        "identity material goes after the queued data"
    );
    // the review's state: the salt gone, the spool preserved
    std::fs::remove_file(dir.join("salt")).unwrap();
    // restart: a producer spools nothing
    assert!(spool
        .enqueue(&consultation_of(TEST_INSTANCE, "decision"))
        .is_err());
    // the sender posts nothing of the old instance - it finishes the cleanup instead
    let r = Spool::new(&dir, hub.clone())
        .flush_with(|b| panic!("posted while a deletion's cleanup was due: {b}"))
        .unwrap();
    assert!(!r.attempted, "{r:?}");
    assert!(
        r.skipped.contains(&format!(
            "finished the local deletion of instance {old_iid}"
        )),
        "{r:?}"
    );
    assert!(!spool_path.exists() && !dir.join("refs.ndjson").exists());
    assert!(!dir.join("forget-pending.json").exists());
    assert_eq!(
        log.lock().unwrap().clone(),
        [format!("DELETE /T/v2/instances/{old_iid}?public_ref=CC-7Q")],
        "the intake saw the DELETE and never an event of the old instance"
    );
    // afterwards: a NEW instance, whose event goes out and never names the old one
    let new_iid = telemetry::instance_id_in(&dir);
    assert_ne!(new_iid, old_iid);
    spool
        .enqueue(&consultation_of(&new_iid, "decision"))
        .unwrap();
    let sent = std::sync::Mutex::new(String::new());
    let r2 = spool
        .flush_with(|b| {
            *sent.lock().unwrap() = b.to_string();
            true
        })
        .unwrap();
    assert_eq!(r2.sent, 1);
    let body = sent.lock().unwrap().clone();
    assert!(
        body.contains(&new_iid) && !body.contains(&old_iid),
        "{body}"
    );
}

/// (F09-3) A `confirmed` transaction left by a process that died right after the intake confirmed
/// (no cleanup started): the next forget - even one without a reference - finishes the cleanup
/// with the transaction's identity, asks the intake nothing more and asks the user nothing; a salt
/// made meanwhile by ANOTHER instance stays.
#[test]
fn a_confirmed_transaction_is_finished_by_the_next_forget() {
    let dir = temp_dir("rc2-forget-resume");
    let old_iid = telemetry::instance_id_in(&dir);
    let spool = Spool::new(&dir, "http://unused.invalid/T");
    spool
        .enqueue(&consultation_of(&old_iid, "framing"))
        .unwrap();
    let record = serde_json::json!({"instance_id": old_iid, "public_ref": "CC-7Q", "since": "2026-10-09T10:00:00Z", "attempts": 1, "last_error": "", "phase": "confirmed"});
    std::fs::write(dir.join("forget-pending.json"), record.to_string()).unwrap();
    let (hub, log) = scripted_mock(vec![(500, "{}")]);
    let out = telemetry::forget_at(&hub_at(&hub), &dir, &forget_req(None, false), |_| {
        panic!("a confirmed deletion asks nothing")
    });
    assert_eq!(out.exit, 0, "{:?}", out.lines);
    let t = out.lines.join("\n");
    assert!(
        t.contains(&format!(
            "finishing the local deletion of instance {old_iid}"
        )),
        "{t}"
    );
    assert!(t.contains("removed locally - spool.ndjson, salt"), "{t}");
    assert!(!dir.join("salt").exists() && spool.pending() == 0);
    assert!(telemetry::pending_deletion_in(&dir).is_none());
    assert!(
        log.lock().unwrap().is_empty(),
        "nothing more asked of the intake"
    );
    // a salt of ANOTHER instance (made after the deletion began) is not the transaction's: kept
    let other_iid = telemetry::instance_id_in(&dir);
    let record = serde_json::json!({"instance_id": old_iid, "public_ref": "CC-7Q", "since": "2026-10-09T10:00:00Z", "attempts": 1, "last_error": "", "phase": "cleaning"});
    std::fs::write(dir.join("forget-pending.json"), record.to_string()).unwrap();
    let again = telemetry::forget_at(&hub_at(&hub), &dir, &forget_req(None, true), |_| true);
    assert_eq!(again.exit, 0, "{:?}", again.lines);
    assert!(
        again
            .lines
            .join("\n")
            .contains(&format!("another instance ({other_iid})")),
        "{:?}",
        again.lines
    );
    assert_eq!(
        telemetry::instance_id_if_exists_in(&dir).as_deref(),
        Some(other_iid.as_str())
    );
    assert!(telemetry::pending_deletion_in(&dir).is_none());
}

// --------------------------------------------------------------------------- wave 2f (F14-1)

/// A telemetry directory with a salt, one queued event of its instance and a stored reference.
fn local_state(tag: &str) -> (PathBuf, String, Spool) {
    let dir = temp_dir(tag);
    let iid = telemetry::instance_id_in(&dir);
    let spool = Spool::new(&dir, "http://unused.invalid/T");
    spool.enqueue(&consultation_of(&iid, "framing")).unwrap();
    std::fs::write(dir.join("refs.ndjson"), "{\"public_ref\":\"CC-7Q\"}\n").unwrap();
    (dir, iid, spool)
}

/// (F14-1 / RC1) A LOCAL-ONLY forget (no reference, no transaction recorded) whose cleanup is
/// interrupted where the spool goes (an injected removal failure - the process dies there): its
/// transaction is on disk - recorded `cleaning` BEFORE the first removal, with the instance id and
/// no reference -, the message about it is true, a producer spools nothing and the sender posts
/// NOTHING; the next flush - or the next forget - finishes the cleanup with that identity, removes
/// the record last and asks the intake nothing.
#[test]
fn a_local_only_forget_interrupted_keeps_its_record_and_resumes_rc1() {
    for resume_by_flush in [true, false] {
        let (dir, iid, spool) = local_state(if resume_by_flush {
            "rc1-local-flush"
        } else {
            "rc1-local-forget"
        });
        let (hub, log) = scripted_mock(vec![(500, "{}")]);
        let spool_path = dir.join("spool.ndjson");
        let phases = std::sync::Mutex::new(Vec::<String>::new());
        let out = telemetry::forget_with(
            &hub_at(&hub),
            &dir,
            &forget_req(None, true),
            |_| panic!("-Yes asks nothing"),
            &|p: &std::path::Path| {
                phases.lock().unwrap().push(
                    telemetry::pending_deletion_in(&dir)
                        .map(|t| t.phase)
                        .unwrap_or_default(),
                );
                if p == spool_path {
                    Err(std::io::Error::other("injected: the process died here"))
                } else {
                    std::fs::remove_file(p)
                }
            },
        );
        assert_eq!(out.exit, 1, "{:?}", out.lines);
        assert!(!out.server_requested && out.pending);
        let t = out.lines.join("\n");
        assert!(t.contains("the local deletion did not finish"), "{t}");
        assert!(
            t.contains(&format!(
                "forget-pending.json keeps instance {iid} - nothing is spooled or sent"
            )),
            "{t}"
        );
        assert_eq!(
            phases.lock().unwrap().clone(),
            ["cleaning"],
            "the transaction is recorded before the first removal"
        );
        let txn = telemetry::pending_deletion_in(&dir).expect("the local-only transaction");
        assert_eq!(txn.phase, telemetry::PHASE_CLEANING);
        assert_eq!(
            (txn.instance_id.as_str(), txn.public_ref.as_str()),
            (iid.as_str(), "")
        );
        assert_eq!(spool.pending(), 1, "the event is still queued");
        assert!(dir.join("salt").is_file() && dir.join("refs.ndjson").is_file());
        // a producer spools nothing
        assert!(spool.enqueue(&consultation_of(&iid, "decision")).is_err());
        if resume_by_flush {
            // the sender posts nothing - it finishes the cleanup instead
            let r = Spool::new(&dir, hub.clone())
                .flush_with(|b| panic!("posted while a local deletion's cleanup was due: {b}"))
                .unwrap();
            assert!(!r.attempted, "{r:?}");
            assert!(
                r.skipped
                    .contains(&format!("finished the local deletion of instance {iid}")),
                "{r:?}"
            );
        } else {
            let again = telemetry::forget_at(&hub_at(&hub), &dir, &forget_req(None, true), |_| {
                panic!("a resumed cleanup asks nothing")
            });
            assert_eq!(again.exit, 0, "{:?}", again.lines);
            assert!(!again.pending);
            let t = again.lines.join("\n");
            assert!(
                t.contains(&format!(
                    "finishing the local deletion of instance {iid} (a local-only deletion that did not finish)"
                )),
                "{t}"
            );
            assert!(
                t.contains("removed locally - spool.ndjson, refs.ndjson, salt"),
                "{t}"
            );
        }
        assert!(!spool_path.exists() && !dir.join("refs.ndjson").exists());
        assert!(!dir.join("salt").exists());
        assert!(
            !dir.join("forget-pending.json").exists(),
            "the record is removed last"
        );
        assert!(
            log.lock().unwrap().is_empty(),
            "a local-only deletion asks the intake nothing"
        );
    }
}

/// (F14-1) A local-only forget that finishes leaves nothing behind: every removal ran under its
/// `cleaning` record and the record went last; the next event is a new instance's and goes out. A
/// machine with nothing local (no salt, no spool) finishes the same way, without an instance id.
#[test]
fn a_local_only_forget_leaves_no_record_behind() {
    let (dir, iid, spool) = local_state("f14-local-done");
    let (hub, log) = scripted_mock(vec![(500, "{}")]);
    let phases = std::sync::Mutex::new(Vec::<String>::new());
    let out = telemetry::forget_with(
        &hub_at(&hub),
        &dir,
        &forget_req(None, true),
        |_| true,
        &|p: &std::path::Path| {
            phases.lock().unwrap().push(
                telemetry::pending_deletion_in(&dir)
                    .map(|t| t.phase)
                    .unwrap_or_default(),
            );
            std::fs::remove_file(p)
        },
    );
    assert_eq!(out.exit, 0, "{:?}", out.lines);
    assert!(!out.pending);
    assert_eq!(out.instance_id, iid);
    assert_eq!(out.removed, ["spool.ndjson", "refs.ndjson", "salt"]);
    assert_eq!(
        phases.lock().unwrap().clone(),
        ["cleaning", "cleaning", "cleaning"]
    );
    assert!(!dir.join("forget-pending.json").exists());
    assert!(telemetry::pending_deletion_in(&dir).is_none());
    // the next event: a new instance, spooled and sent
    let new_iid = telemetry::instance_id_in(&dir);
    assert_ne!(new_iid, iid);
    spool
        .enqueue(&consultation_of(&new_iid, "decision"))
        .unwrap();
    assert_eq!(spool.flush_with(|_| true).unwrap().sent, 1);
    // nothing local at all: no instance id, nothing removed, no record left
    let empty = temp_dir("f14-local-empty");
    let none = telemetry::forget_at(&hub_at(&hub), &empty, &forget_req(None, true), |_| true);
    assert_eq!(none.exit, 0, "{:?}", none.lines);
    assert!(none.instance_id.is_empty() && none.removed.is_empty() && !none.pending);
    assert!(none
        .lines
        .join("\n")
        .contains("removed locally - nothing (there was no spool and no salt)"));
    assert!(!empty.join("forget-pending.json").exists());
    assert!(log.lock().unwrap().is_empty());
}

/// (F14-1) A local-only forget whose transaction cannot be recorded (here: a directory stands
/// where the record goes) removes NOTHING - a cleanup without its record could be interrupted
/// with nothing to block or resume it.
#[test]
fn a_local_only_forget_that_cannot_record_removes_nothing() {
    let (dir, _iid, spool) = local_state("f14-local-norecord");
    std::fs::create_dir(dir.join("forget-pending.json")).unwrap();
    let (hub, _log) = scripted_mock(vec![(500, "{}")]);
    let out = telemetry::forget_with(
        &hub_at(&hub),
        &dir,
        &forget_req(None, true),
        |_| true,
        &|p: &std::path::Path| panic!("removed {} without a record", p.display()),
    );
    assert_eq!(out.exit, 1, "{:?}", out.lines);
    assert!(
        out.lines
            .join("\n")
            .contains("the local deletion could not be recorded before its cleanup"),
        "{:?}",
        out.lines
    );
    assert!(out.removed.is_empty());
    assert!(dir.join("salt").is_file() && dir.join("refs.ndjson").is_file());
    assert_eq!(spool.pending(), 1);
}

/// (F09-4 / RC3) The sender delivered A; B was appended during the POST; the reread of the
/// CURRENT spool fails: the flush reports the failure and the spool's bytes are exactly what they
/// were (A and B) - nothing is replaced from a guess (the failure used to read as an empty file and
/// erase both). The next flush re-sends A (at worst a duplicate) and delivers B.
#[test]
fn outbox_reread_failure_after_a_send_keeps_the_spool_bytes_rc3() {
    let dir = temp_dir("rc3-reread");
    let spool = Spool::new(&dir, "http://unused.invalid/T");
    enqueue_one(&spool, "framing");
    let path = dir.join("spool.ndjson");
    let before = std::sync::Mutex::new(Vec::new());
    let fail = |_: &std::path::Path| -> std::io::Result<String> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "injected reread failure",
        ))
    };
    let r = spool.flush_hooked(
        |body| {
            assert!(body.contains("\"framing\""));
            enqueue_one(&spool, "decision"); // B arrives while A's POST is in flight
            *before.lock().unwrap() = std::fs::read(&path).unwrap();
            true
        },
        &telemetry::FlushHooks {
            reread: Some(&fail),
            ..Default::default()
        },
    );
    let err = r.expect_err("a failed reread is a failed flush");
    assert!(err.to_string().contains("could not be re-read"), "{err}");
    let bytes = before.lock().unwrap().clone();
    assert!(!bytes.is_empty());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        bytes,
        "the spool bytes are unchanged"
    );
    assert_eq!(spool.pending(), 2);
    let next = spool.flush_with(|_| true).unwrap();
    assert_eq!(next.sent, 2);
    assert_eq!(spool.pending(), 0);
}

/// (F09-5 / RC4) The upgrade backlog: a FRESH raw event queued before wave 2 (provider, model,
/// purpose, title, tags and outcome the operator's labels as typed - `customer-acme`), a rating of
/// that time, and a wave-2b envelope whose body still carries a label all leave through the closed
/// classes - `customer-acme` never reaches the intake, the values that ARE closed survive; an event
/// that cannot be closed (no instance id) is discarded unsent, with a diagnostic.
#[test]
fn legacy_queued_events_never_send_a_private_label_rc4() {
    let dir = temp_dir("rc4-legacy");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let consult = serde_json::json!({"app_id": "c3", "app_version": "0.1.0", "instance_id": TEST_INSTANCE, "event_type": "consultation", "severity": "info", "title": "customer-acme-intake",
        "details": {"engine": "codex", "provider": "customer-acme", "model": "customer-acme-7b", "purpose": "customer-acme-intake", "outcome": "failed:customer-acme-gateway", "wall_seconds": 12.5, "tokens_in": 10, "tokens_out": 5, "findings": 2, "structured": true, "format_retry": false, "panel_size": 1, "peers_used": 0, "os": "Windows", "runtime": "rust 0.1.0"},
        "client_time": now, "os": "Windows", "runtime": "rust 0.1.0", "tags": ["customer-acme"]});
    let rating = serde_json::json!({"app_id": "c3", "app_version": "0.1.0", "instance_id": TEST_INSTANCE, "event_type": "rating", "severity": "info", "title": "customer-acme-intake",
        "details": {"engine": "codex", "provider": "openai", "model": "GPT-5.1", "purpose": "framing", "mark": "yes", "age_days": 2, "os": "Windows", "runtime": "rust 0.1.0", "topic_tags": ["security"]},
        "client_time": now, "os": "Windows", "runtime": "rust 0.1.0", "tags": []});
    let no_id = serde_json::json!({"app_id": "c3", "event_type": "consultation", "client_time": now, "details": {"provider": "customer-acme"}});
    let envelope_body = serde_json::json!({"app_id": "c3", "app_version": "0.1.0", "instance_id": TEST_INSTANCE, "event_type": "consultation", "severity": "info", "title": "framing",
        "details": {"engine": "codex", "provider": "ZAI-customer-acme", "model": "glm-5.3", "purpose": "framing", "outcome": "usable", "wall_seconds": 1.0, "tokens_in": 1, "tokens_out": 1, "findings": 0, "structured": true, "format_retry": false, "panel_size": 1, "peers_used": 0, "os": "Windows", "runtime": "rust 0.1.0"},
        "tags": ["ZAI-customer-acme", "glm-5.3"], "client_time": now, "os": "Windows", "runtime": "rust 0.1.0"});
    let envelope = serde_json::json!({"v": 1, "kind": "event", "queued_unix": chrono::Utc::now().timestamp(), "body": envelope_body.to_string()});
    std::fs::write(
        dir.join("spool.ndjson"),
        format!("{consult}\n{rating}\n{no_id}\n{envelope}\n"),
    )
    .unwrap();
    let spool = Spool::new(&dir, "http://unused.invalid/T");
    let posted = std::sync::Mutex::new(String::new());
    let r = spool
        .flush_with(|b| {
            *posted.lock().unwrap() = b.to_string();
            true
        })
        .unwrap();
    assert_eq!((r.sent, r.discarded), (3, 1), "{r:?}");
    assert_eq!(spool.pending(), 0);
    let body = posted.lock().unwrap().clone();
    assert!(!body.to_lowercase().contains("acme"), "{body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let evs = v["events"].as_array().unwrap();
    assert_eq!(evs.len(), 3);
    // the raw consultation: every label closed, the counts kept, the severity from the outcome
    let c = &evs[0];
    assert_eq!(c["details"]["provider"], "other");
    assert_eq!(c["details"]["model"], "other");
    assert_eq!(c["details"]["purpose"], "other");
    assert_eq!(c["details"]["outcome"], "failed:unknown");
    assert_eq!(c["title"], "other");
    assert_eq!(c["severity"], "error");
    assert_eq!(c["tags"], serde_json::json!(["other", "other"]));
    assert_eq!(c["details"]["tokens_in"], 10);
    assert_eq!(c["details"]["wall_seconds"], 12.5);
    assert_eq!(c["instance_id"], TEST_INSTANCE);
    // the raw rating: the closed values survive (the table's spelling), the 0.6.1 detail keys
    let g = &evs[1];
    assert_eq!(g["details"]["provider"], "openai");
    assert_eq!(g["details"]["model"], "gpt-5.1");
    assert_eq!(g["details"]["purpose"], "framing");
    assert_eq!(g["title"], "yes");
    assert_eq!(
        g["details"]["judge"],
        serde_json::json!({"provider": "other", "model": "other", "source": "unknown"})
    );
    let keys: Vec<&str> = g["details"]
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    assert_eq!(
        keys,
        [
            "engine",
            "provider",
            "model",
            "purpose",
            "mark",
            "age_days",
            "bridge_version",
            "os",
            "ps_version",
            "judge"
        ]
    );
    // the 2b envelope: its label closed, its listed model kept only with a class (none here)
    assert_eq!(evs[2]["details"]["provider"], "other");
    assert_eq!(evs[2]["details"]["model"], "other");
    assert_eq!(evs[2]["details"]["outcome"], "usable");
}

// --------------------------------------------------------------------------- wave 2g (F19-1..F19-3)

/// A complaint in a thread whose confirmation callback is a barrier: it hands over the payload it
/// shows, then waits for the answer (`true`: confirmed).
fn complaint_at_barrier(
    hub: &str,
    dir: &std::path::Path,
) -> (
    thread::JoinHandle<telemetry::Result<Option<String>>>,
    Receiver<String>,
    std::sync::mpsc::Sender<bool>,
) {
    let (shown, payload) = channel::<String>();
    let (answer, answered) = channel::<bool>();
    let (h, d) = (hub.to_string(), dir.to_path_buf());
    let complaint = thread::spawn(move || {
        telemetry::complain_to(&h, &d, "the panel hung on peak windows", None, |p| {
            shown.send(p.to_string()).unwrap();
            answered.recv().unwrap()
        })
    });
    (complaint, payload, answer)
}

/// (F19-1 / RC1) A complaint shows the payload of instance A and waits in its confirmation (a
/// barrier); meanwhile a forget is CONFIRMED by the intake and removes A locally; then the user
/// confirms: nothing is posted under A, no reference is written into the cleared directory and no
/// new instance is made. Again with a forget whose DELETE fails (a pending deletion): refused, no
/// POST, no reference added. A complaint begun while a transaction exists (pending, or a cleanup
/// not finished) is refused before its question.
#[test]
fn a_complaint_confirmed_after_a_forget_never_posts_the_old_instance_rc1() {
    // schedule 1: a successful remote-plus-local forget while the complaint waits
    let (dir, iid, _spool) = local_state("rc1-complaint-forget");
    let (hub, log) = scripted_mock(vec![(
        200,
        r#"{"ok":true,"deleted":3,"public_ref":"T-LEAK-0001"}"#,
    )]);
    let (complaint, payload, answer) = complaint_at_barrier(&hub, &dir);
    let shown = payload.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(shown.contains(&iid), "{shown}");
    let out = telemetry::forget_at(
        &hub_at(&hub),
        &dir,
        &forget_req(Some("CC-7Q"), true),
        |_| true,
    );
    assert!(out.server_deleted && out.exit == 0, "{:?}", out.lines);
    assert!(!dir.join("salt").exists() && !dir.join("refs.ndjson").exists());
    answer.send(true).unwrap();
    let e = complaint
        .join()
        .unwrap()
        .expect_err("a complaint of a forgotten instance is refused")
        .to_string();
    assert!(
        e.contains(&format!("names instance {iid}"))
            && e.contains("is gone")
            && e.contains("nothing was sent"),
        "{e}"
    );
    assert_eq!(
        log.lock().unwrap().clone(),
        [format!("DELETE /T/v2/instances/{iid}?public_ref=CC-7Q")],
        "no POST under the forgotten identity"
    );
    assert!(
        !dir.join("refs.ndjson").exists(),
        "no reference written into the cleared directory"
    );
    assert!(!dir.join("salt").exists(), "the refusal makes no instance");
    assert!(telemetry::pending_deletion_in(&dir).is_none());

    // schedule 2: the forget's DELETE fails while the complaint waits - a pending deletion
    let (dir, iid, spool) = local_state("rc1-complaint-pending");
    let (hub, log) = scripted_mock(vec![(
        503,
        r#"{"ok":false,"error":"maintenance","public_ref":"T-LEAK-0002"}"#,
    )]);
    let (complaint, payload, answer) = complaint_at_barrier(&hub, &dir);
    let shown = payload.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(shown.contains(&iid), "{shown}");
    let out = telemetry::forget_at(
        &hub_at(&hub),
        &dir,
        &forget_req(Some("CC-7Q"), true),
        |_| true,
    );
    assert_eq!(out.exit, 3, "{:?}", out.lines);
    answer.send(true).unwrap();
    let e = complaint
        .join()
        .unwrap()
        .expect_err("a complaint during a pending deletion is refused")
        .to_string();
    assert!(
        e.contains("pending at the intake (it began while the complaint waited")
            && e.contains("nothing was sent"),
        "{e}"
    );
    let delete = format!("DELETE /T/v2/instances/{iid}?public_ref=CC-7Q");
    assert_eq!(*log.lock().unwrap(), std::slice::from_ref(&delete));
    assert_eq!(
        std::fs::read_to_string(dir.join("refs.ndjson")).unwrap(),
        "{\"public_ref\":\"CC-7Q\"}\n",
        "no reference added"
    );
    assert_eq!(spool.pending(), 1, "the pending deletion keeps the outbox");

    // schedule 3: begun while a transaction exists - refused before the question
    for phase in ["pending", "cleaning"] {
        let record = serde_json::json!({"instance_id": iid, "public_ref": "CC-7Q", "since": "2026-10-09T10:00:00Z", "attempts": 1, "last_error": "", "phase": phase});
        std::fs::write(dir.join("forget-pending.json"), record.to_string()).unwrap();
        let e = telemetry::complain_to(&hub, &dir, "text", None, |_| {
            panic!("asked during a deletion ({phase})")
        })
        .expect_err("refused")
        .to_string();
        let why = if phase == "pending" {
            "pending at the intake ("
        } else {
            "is not finished ("
        };
        assert!(e.contains(why) && e.contains("nothing was sent"), "{e}");
    }
    assert_eq!(
        log.lock().unwrap().clone(),
        [delete],
        "still only the DELETE"
    );
}

/// A handle that denies every other reader of a file while its metadata stays visible: on Windows
/// the file held open with share mode 0 (RC2's case), elsewhere its permissions `0o000` (restored
/// on drop).
struct DenyRead {
    #[cfg(windows)]
    _file: std::fs::File,
    #[cfg(not(windows))]
    path: PathBuf,
}

#[cfg(not(windows))]
impl Drop for DenyRead {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
    }
}

/// Deny reading `path` ([`DenyRead`]); `None` when it can still be read (a privileged user off
/// Windows).
fn deny_reading(path: &std::path::Path) -> Option<DenyRead> {
    #[cfg(windows)]
    let held = {
        use std::os::windows::fs::OpenOptionsExt;
        DenyRead {
            _file: std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(path)
                .unwrap(),
        }
    };
    #[cfg(not(windows))]
    let held = {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
        DenyRead {
            path: path.to_path_buf(),
        }
    };
    std::fs::read(path).is_err().then_some(held)
}

/// (F19-2 / RC2) A confirmed deletion's cleanup resumed while another handle denies reading the
/// salt (its metadata visible): the salt is neither removed nor kept as "another instance" - the
/// cleanup FAILS and the transaction stays (spooling and sending blocked), by the sender and by a
/// forget alike; a local-only forget begun then is refused before it records or removes anything.
/// Once the handle is released the cleanup removes the old salt - the record still blocking - and
/// only then the record; the next event is a new instance.
#[test]
fn a_cleanup_never_clears_its_transaction_while_the_salt_cannot_be_read_rc2() {
    let (dir, iid, spool) = local_state("rc2-unreadable-salt");
    let salt = dir.join("salt");
    let Some(handle) = deny_reading(&salt) else {
        eprintln!("skipped: this user reads a file whose permissions deny it");
        return;
    };
    assert!(salt.exists(), "the salt's metadata stays visible");
    let nowhere = hub_at("http://unused.invalid/T");
    // a local-only forget cannot name the instance now: refused, nothing recorded or removed
    let out = telemetry::forget_at(&nowhere, &dir, &forget_req(None, true), |_| true);
    assert_eq!(out.exit, 1, "{:?}", out.lines);
    assert!(
        out.lines.join("\n").contains("exists but cannot be read"),
        "{:?}",
        out.lines
    );
    assert!(telemetry::pending_deletion_in(&dir).is_none());
    assert_eq!(spool.pending(), 1);
    // the intake confirmed the deletion of this instance; its local cleanup is due
    let record = serde_json::json!({"instance_id": iid, "public_ref": "CC-7Q", "since": "2026-10-09T10:00:00Z", "attempts": 1, "last_error": "", "phase": "confirmed"});
    std::fs::write(dir.join("forget-pending.json"), record.to_string()).unwrap();
    // resumed by the sender: the cleanup fails at the salt, nothing is posted, the record stays
    let r = Spool::new(&dir, "http://unused.invalid/T")
        .flush_with(|b| panic!("posted during a deletion: {b}"))
        .unwrap();
    assert!(!r.attempted, "{r:?}");
    assert!(
        r.skipped.contains(&format!(
            "the local deletion of instance {iid} is not finished"
        )) && r.skipped.contains("cannot be read"),
        "{r:?}"
    );
    let txn = telemetry::pending_deletion_in(&dir).expect("the transaction is retained");
    assert_eq!(txn.phase, telemetry::PHASE_CLEANING);
    assert_eq!(txn.instance_id, iid);
    assert!(salt.exists(), "an unreadable salt is not removed");
    assert_eq!(spool.pending(), 0, "the queued data went first");
    assert!(spool.enqueue(&consultation_of(&iid, "decision")).is_err());
    // resumed by a forget: the same
    let out = telemetry::forget_at(&nowhere, &dir, &forget_req(None, false), |_| {
        panic!("a cleanup that is due asks nothing")
    });
    assert_eq!(out.exit, 1, "{:?}", out.lines);
    let t = out.lines.join("\n");
    assert!(
        t.contains("cannot be read") && t.contains(&format!("keeps instance {iid}")),
        "{t}"
    );
    assert!(!t.contains("the salt names another instance"), "{t}");
    assert!(telemetry::pending_deletion_in(&dir).is_some());
    // the handle released: the old salt goes while the record still blocks, then the record
    drop(handle);
    let removals = std::sync::Mutex::new(Vec::<(String, bool)>::new());
    let pending_path = dir.join("forget-pending.json");
    let out = telemetry::forget_with(
        &nowhere,
        &dir,
        &forget_req(None, false),
        |_| panic!("a cleanup that is due asks nothing"),
        &|p: &std::path::Path| {
            removals.lock().unwrap().push((
                p.file_name().unwrap().to_string_lossy().to_string(),
                pending_path.exists(),
            ));
            std::fs::remove_file(p)
        },
    );
    assert_eq!(out.exit, 0, "{:?}", out.lines);
    assert_eq!(
        removals.lock().unwrap().last().cloned(),
        Some(("salt".to_string(), true)),
        "the salt is removed before the record that blocks telemetry"
    );
    assert!(!salt.exists() && !pending_path.exists());
    // unblocked: the next event is a new instance
    let new_iid = telemetry::instance_id_in(&dir);
    assert_ne!(new_iid, iid);
    spool
        .enqueue(&consultation_of(&new_iid, "decision"))
        .unwrap();
}

/// (F19-3 / RC3) A current valid event in the outbox with `"title":"customer-acme",` inserted
/// before its real title (a duplicate key: the parse keeps the last one): what `flush_with` sends
/// never carries the private string - the event leaves as its serialised reconstruction, byte for
/// byte the event as it was built.
#[test]
fn a_duplicate_private_title_never_leaves_the_outbox_rc3() {
    let dir = temp_dir("rc3-duplicate-key");
    let iid = telemetry::instance_id_in(&dir);
    let spool = Spool::new(&dir, "http://unused.invalid/T");
    spool.enqueue(&consultation_of(&iid, "framing")).unwrap();
    let path = dir.join("spool.ndjson");
    let mut line: serde_json::Value =
        serde_json::from_str(std::fs::read_to_string(&path).unwrap().trim()).unwrap();
    let body = line["body"].as_str().unwrap().to_string();
    let forged = body.replacen("\"title\":", "\"title\":\"customer-acme\",\"title\":", 1);
    assert!(forged != body && forged.contains("customer-acme"));
    line["body"] = serde_json::json!(forged);
    std::fs::write(&path, format!("{line}\n")).unwrap();
    let sent = std::sync::Mutex::new(String::new());
    let r = spool
        .flush_with(|b| {
            *sent.lock().unwrap() = b.to_string();
            true
        })
        .unwrap();
    let out = sent.lock().unwrap().clone();
    assert!(!out.contains("customer-acme"), "{out}");
    assert_eq!(r.sent, 1, "{r:?}");
    assert_eq!(out, format!("{{\"events\":[{body}]}}"));
}

// --------------------------------------------------------------------------- off switch

#[test]
fn off_switch_flag_disables() {
    // The flag path does not touch the process environment (safe under parallel tests).
    assert!(!telemetry::is_enabled(&Config {
        telemetry: Some(false),
    }));
    assert!(telemetry::is_enabled(&Config {
        telemetry: Some(true),
    }));
    let status = telemetry::status(&Config {
        telemetry: Some(false),
    });
    assert_eq!(status, "telemetry: off (--telemetry off)");
}

// --------------------------------------------------------------------------- wave 5: the sender's parity

/// A scripted intake with headers: answers `(code, content type, body, headers)` in order (the last
/// one repeats) and records every request body with the time it arrived. Returns the `.../T` base.
type Answer = (u16, &'static str, String, Vec<(&'static str, String)>);
type Requests = std::sync::Arc<std::sync::Mutex<Vec<(std::time::Instant, String)>>>;
fn intake(answers: Vec<Answer>) -> (String, Requests) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let log: Requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log2 = log.clone();
    thread::spawn(move || {
        for (k, stream) in listener.incoming().enumerate() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut content_length = 0usize;
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_length = v.trim().parse().unwrap_or(0);
                }
            }
            let mut buf = vec![0u8; content_length];
            let _ = reader.read_exact(&mut buf);
            log2.lock().unwrap().push((
                std::time::Instant::now(),
                String::from_utf8_lossy(&buf).to_string(),
            ));
            let (code, ctype, body, headers) = &answers[k.min(answers.len() - 1)];
            let extra: String = headers
                .iter()
                .map(|(n, v)| format!("{n}: {v}\r\n"))
                .collect();
            let resp = format!(
                "HTTP/1.1 {code} X\r\ncontent-type: {ctype}\r\ncontent-length: {}\r\n{extra}connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    (format!("http://{addr}/T"), log)
}

const OK: &str = r#"{"ok":true,"accepted":1,"event_ids":[]}"#;

fn ok_answer() -> Answer {
    (200, "application/json", OK.to_string(), vec![])
}

fn answer_429(retry_after: &str) -> Answer {
    (
        429,
        "application/json",
        r#"{"ok":false,"error":"rate"}"#.to_string(),
        vec![("Retry-After", retry_after.to_string())],
    )
}

fn last_record(dir: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("last-flush.json")).unwrap()).unwrap()
}

/// The purposes of a posted batch, in order.
fn purposes(body: &str) -> Vec<String> {
    let v: serde_json::Value = serde_json::from_str(body).unwrap();
    v["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["details"]["purpose"].as_str().unwrap_or("").to_string())
        .collect()
}

/// (harness-telemetry R429 #1) A 429 with `Retry-After: 1` is honoured: the SAME request is sent
/// once more after at least that long, delivered; the spool is emptied.
#[test]
fn sender_429_with_a_short_retry_after_resends_the_same_batch_once() {
    let dir = temp_dir("s429");
    let (hub, reqs) = intake(vec![answer_429("1"), ok_answer()]);
    let spool = Spool::new(&dir, hub);
    enqueue_one(&spool, "framing");
    let r = spool.flush_recorded().unwrap();
    let reqs = reqs.lock().unwrap();
    assert_eq!(reqs.len(), 2, "{r:?}");
    assert_eq!(reqs[0].1, reqs[1].1, "the same body");
    let gap = reqs[1].0 - reqs[0].0;
    assert!(gap >= Duration::from_millis(950), "{gap:?}");
    assert_eq!((r.sent, r.http, r.stopped.as_str()), (1, Some(200), ""));
    assert_eq!(spool.pending(), 0);
    let last = last_record(&dir);
    assert_eq!(last["result"], "delivered 1, kept 0, dropped 0", "{last}");
    assert_eq!(last["http"], 200);
}

/// (harness-telemetry R429 #2) A 429 whose `Retry-After` is more than 60 s (or missing) is NOT
/// waited for: one request, not delivered, the spool byte-identical, the record names the 429.
#[test]
fn sender_429_with_a_long_or_no_retry_after_keeps_the_batch() {
    for (ra, why) in [
        (
            Some("120"),
            "HTTP 429 (Retry-After 120 s, more than 60 s: not retried now)",
        ),
        (None, "HTTP 429 (Retry-After not given: not retried now)"),
    ] {
        let dir = temp_dir("s429long");
        let answer = match ra {
            Some(v) => answer_429(v),
            None => (
                429,
                "application/json",
                r#"{"ok":false}"#.to_string(),
                vec![],
            ),
        };
        let (hub, reqs) = intake(vec![answer]);
        let spool = Spool::new(&dir, hub);
        enqueue_one(&spool, "framing");
        let before = std::fs::read(dir.join("spool.ndjson")).unwrap();
        let r = spool.flush_recorded().unwrap();
        assert_eq!(reqs.lock().unwrap().len(), 1);
        assert_eq!(r.stopped, why);
        assert_eq!(r.sent, 0);
        assert_eq!(std::fs::read(dir.join("spool.ndjson")).unwrap(), before);
        let last = last_record(&dir);
        assert_eq!(
            last["result"].as_str().unwrap(),
            format!("not delivered: {why} - delivered 0, kept 1, dropped 0")
        );
        assert_eq!(last["http"], 429);
    }
}

/// A second 429 after the wait stops the flush (no third request); a `Retry-After` that does not
/// fit into what is left of the flush's deadline is not waited for.
#[test]
fn sender_429_again_or_not_fitting_the_deadline_stops() {
    let dir = temp_dir("s429again");
    let (hub, reqs) = intake(vec![answer_429("0")]);
    let spool = Spool::new(&dir, hub);
    enqueue_one(&spool, "framing");
    let r = spool.flush().unwrap();
    assert_eq!(reqs.lock().unwrap().len(), 2);
    assert_eq!(r.stopped, "HTTP 429 again after its Retry-After");
    assert_eq!(spool.pending(), 1);

    let dir = temp_dir("s429fit");
    let (hub, reqs) = intake(vec![answer_429("5"), ok_answer()]);
    let spool = Spool::new(&dir, hub).with_limits(Duration::from_secs(4), Duration::from_secs(2));
    enqueue_one(&spool, "framing");
    let r = spool.flush().unwrap();
    assert_eq!(reqs.lock().unwrap().len(), 1);
    assert_eq!(
        r.stopped,
        "HTTP 429 (Retry-After 5 s does not fit into the flush's deadline: not retried now)"
    );
    assert_eq!(spool.pending(), 1);
}

/// (D8) A 400 naming `events[1]: <reason>` drops that event (one `rejected` line, counted as
/// dropped) and resends the rest; the spool is emptied - the refused event is never resent.
#[test]
fn sender_400_drops_the_named_event_and_resends_the_rest() {
    let dir = temp_dir("s400");
    let (hub, reqs) = intake(vec![
        (
            400,
            "application/json",
            r#"{"ok":false,"error":"events[1]: title longer than 200 characters"}"#.to_string(),
            vec![],
        ),
        ok_answer(),
    ]);
    let spool = Spool::new(&dir, hub);
    for p in ["framing", "decision", "checkpoint"] {
        enqueue_one(&spool, p);
    }
    let r = spool.flush_recorded().unwrap();
    let reqs = reqs.lock().unwrap();
    assert_eq!(reqs.len(), 2);
    assert_eq!(purposes(&reqs[0].1), ["framing", "decision", "checkpoint"]);
    assert_eq!(purposes(&reqs[1].1), ["framing", "checkpoint"]);
    assert_eq!((r.sent, r.rejected.len(), r.dropped()), (2, 1, 1));
    assert!(
        r.rejected[0].starts_with("event queued ")
            && r.rejected[0].ends_with(" refused: title longer than 200 characters"),
        "{:?}",
        r.rejected
    );
    assert_eq!(spool.pending(), 0);
    let last = last_record(&dir);
    assert_eq!(
        last["result"],
        "delivered 2, kept 0, dropped 1 (refused by the intake)"
    );
    assert_eq!(last["dropped"], 1);
    assert_eq!(last["rejected"].as_array().unwrap().len(), 1);
}

/// (D8) At most three per-event refusals per flush: the fourth stops it, the rest stays.
#[test]
fn sender_400_a_fourth_refusal_in_one_flush_stops_it() {
    let dir = temp_dir("s400four");
    let refuse_first = (
        400,
        "application/json",
        r#"{"ok":false,"error":"events[0]: bad"}"#.to_string(),
        vec![],
    );
    let (hub, reqs) = intake(vec![refuse_first]);
    let spool = Spool::new(&dir, hub);
    for _ in 0..5 {
        enqueue_one(&spool, "framing");
    }
    let r = spool.flush().unwrap();
    assert_eq!(reqs.lock().unwrap().len(), 4);
    assert_eq!(r.rejected.len(), 3);
    assert_eq!(
        r.stopped,
        "HTTP 400: events[0]: bad - a fourth refused event in this flush; the rest stays"
    );
    assert_eq!(spool.pending(), 2);
    assert!(r
        .result_text()
        .ends_with("- delivered 0, kept 2, dropped 3, 3 refused by the intake"));
}

/// (D8) 413: the batch is halved until it fits; an event refused alone is dropped
/// ("HTTP 413 (too large alone)"), never resent.
#[test]
fn sender_413_halves_the_batch_and_drops_an_event_too_large_alone() {
    let dir = temp_dir("s413");
    let spool = Spool::new(&dir, "http://unused.invalid/T");
    for p in ["framing", "decision", "checkpoint", "acceptance"] {
        enqueue_one(&spool, p);
    }
    let sizes = std::sync::Mutex::new(Vec::new());
    let r = spool
        .flush_answered(|body| {
            let ps = purposes(body);
            sizes.lock().unwrap().push(ps.len());
            if ps.len() > 1 || ps[0] == "decision" {
                c3::telemetry::PostAnswer::answer(413, "", "", None)
            } else {
                c3::telemetry::PostAnswer::ok()
            }
        })
        .unwrap();
    assert_eq!(*sizes.lock().unwrap(), [4, 2, 1, 1, 1, 1]);
    assert_eq!((r.sent, r.rejected.len()), (3, 1));
    assert!(
        r.rejected[0].ends_with("refused: HTTP 413 (too large alone)"),
        "{:?}",
        r.rejected
    );
    assert_eq!(spool.pending(), 0);
    assert!(r.stopped.is_empty(), "{r:?}");
}

/// (harness-telemetry BATCH) Every fresh event goes, in batches of at most 100, oldest first.
#[test]
fn sender_sends_every_event_in_batches_of_at_most_100() {
    let dir = temp_dir("sbatch");
    let spool = Spool::new(&dir, "http://unused.invalid/T");
    enqueue_one(&spool, "decision");
    for _ in 1..150 {
        enqueue_one(&spool, "framing");
    }
    let sizes = std::sync::Mutex::new(Vec::new());
    let r = spool
        .flush_answered(|body| {
            let ps = purposes(body);
            sizes.lock().unwrap().push((ps.len(), ps[0].clone()));
            c3::telemetry::PostAnswer::ok()
        })
        .unwrap();
    let sizes = sizes.lock().unwrap();
    assert_eq!(sizes.len(), 2);
    assert_eq!(sizes[0], (100, "decision".to_string()), "oldest first");
    assert_eq!(sizes[1].0, 50);
    assert_eq!((r.sent, spool.pending()), (150, 0));
}

/// (wave 28b, D2 / wave 28c, D5) The flush's deadline: a batch starts only while 1.5 s are left;
/// what is not sent stays. (2.5 s: the first batch starts even on a loaded machine - up to 1 s of
/// start-up -, the second cannot after a 1.5 s answer.)
#[test]
fn sender_stops_at_the_flush_deadline_and_keeps_the_rest() {
    let dir = temp_dir("sdeadline");
    let spool = Spool::new(&dir, "http://unused.invalid/T")
        .with_limits(Duration::from_millis(2500), Duration::from_secs(2));
    for _ in 0..150 {
        enqueue_one(&spool, "framing");
    }
    let r = spool
        .flush_answered(|_| {
            thread::sleep(Duration::from_millis(1500));
            c3::telemetry::PostAnswer::ok()
        })
        .unwrap();
    assert_eq!(r.sent, 100);
    assert_eq!(r.stopped, "the flush's deadline (2.5 s) was reached");
    assert_eq!(spool.pending(), 50);
}

/// A 403 stops the flush ("the intake refuses this app"); an answer that is not the intake's JSON
/// is not delivered; another 4xx says "the spool is kept"; nothing is resent in the flush.
#[test]
fn sender_403_non_json_and_other_refusals_stop_and_keep_the_spool() {
    for (answer, why) in [
        (
            (
                403,
                "application/json",
                r#"{"ok":false}"#.to_string(),
                vec![],
            ),
            "HTTP 403 - the intake refuses this app (HTTP 403 without ok: true); the spool is kept",
        ),
        (
            (200, "text/html", "<html>T</html>".to_string(), vec![]),
            "HTTP 200 (text/html) is not the intake's JSON answer",
        ),
        (
            (
                404,
                "application/json",
                r#"{"ok":false,"error":"no route"}"#.to_string(),
                vec![],
            ),
            "HTTP 404: no route - the spool is kept",
        ),
        (
            (
                500,
                "application/json",
                r#"{"ok":false,"error":"boom"}"#.to_string(),
                vec![],
            ),
            "HTTP 500: boom",
        ),
    ] {
        let dir = temp_dir("srefuse");
        let (hub, reqs) = intake(vec![answer]);
        let spool = Spool::new(&dir, hub);
        enqueue_one(&spool, "framing");
        enqueue_one(&spool, "decision");
        let r = spool.flush().unwrap();
        assert_eq!(reqs.lock().unwrap().len(), 1, "{why}");
        assert_eq!(r.stopped, why);
        assert_eq!(spool.pending(), 2);
    }
}

/// (wave 28d, D3) The sender lock's owner record: written whole while a flush holds the lock
/// ({pid, start_time, start_ticks, token, since, host}), gone once it is released; a second sender
/// and `--status` say "busy since <t> (pid <n> ...)".
#[test]
fn sender_owner_record_lives_with_the_lock() {
    let dir = temp_dir("sowner");
    let spool = Spool::new(&dir, "http://unused.invalid/T");
    enqueue_one(&spool, "framing");
    let owner = dir.join("flush.owner.json");
    let (to_test, posted) = std::sync::mpsc::channel::<String>();
    let (to_sender, proceed) = std::sync::mpsc::channel::<()>();
    let d2 = dir.clone();
    let first = thread::spawn(move || {
        Spool::new(&d2, "http://unused.invalid/T")
            .flush_with(|_| {
                to_test
                    .send(std::fs::read_to_string(d2.join("flush.owner.json")).unwrap())
                    .unwrap();
                proceed.recv().unwrap();
                true
            })
            .unwrap()
    });
    let rec: serde_json::Value =
        serde_json::from_str(&posted.recv_timeout(Duration::from_secs(10)).unwrap()).unwrap();
    let keys: Vec<&str> = rec
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    assert_eq!(
        keys,
        ["pid", "start_time", "start_ticks", "token", "since", "host"]
    );
    assert_eq!(rec["pid"], std::process::id());
    let second = spool
        .flush_with(|_| panic!("a second sender must not post"))
        .unwrap();
    assert!(
        second.skipped.starts_with(&format!(
            "another flush is running: sender busy since {} (pid {} holds its lock, ",
            rec["since"].as_str().unwrap(),
            std::process::id()
        )),
        "{second:?}"
    );
    let st = telemetry::sender_status(&dir).unwrap();
    assert!(st.starts_with("busy since "), "{st}");
    to_sender.send(()).unwrap();
    assert_eq!(first.join().unwrap().sent, 1);
    assert!(!owner.exists(), "the record goes with the lock");
    assert!(telemetry::sender_status(&dir).is_none());
}

/// (wave 28d, D3) A lock held for 30 minutes by a living owner: the refused sender says "sender
/// stuck since <t> (pid <n>)" and writes it into the record's notes ONCE however often it is
/// refused; `--status` says it; once the lock is free the next flush drops the note.
#[test]
fn sender_stuck_for_30_minutes_is_noted_once_and_dropped_when_free() {
    let dir = temp_dir("sstuck");
    let spool = Spool::new(&dir, "http://unused.invalid/T");
    enqueue_one(&spool, "framing");
    // a living owner (this process) holds the sender lock; its record is 31 minutes old
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("flush.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let owner = dir.join("flush.owner.json");
    std::fs::write(
        &owner,
        format!(
            "{{\"pid\":{},\"start_time\":\"\",\"token\":\"t\",\"since\":\"2026-10-09T10:00:00+02:00\"}}\n",
            std::process::id()
        ),
    )
    .unwrap();
    std::fs::File::options()
        .write(true)
        .open(&owner)
        .unwrap()
        .set_modified(std::time::SystemTime::now() - Duration::from_secs(31 * 60))
        .unwrap();
    let stuck = format!(
        "sender stuck since 2026-10-09T10:00:00+02:00 (pid {})",
        std::process::id()
    );
    for _ in 0..2 {
        let r = spool
            .flush_recorded_with(|_| panic!("a refused sender must not post"))
            .unwrap();
        assert!(
            r.skipped.starts_with(&format!(
                "another flush is running: {stuck} - its lock is 31 min old"
            )),
            "{r:?}"
        );
    }
    let notes: Vec<String> = last_record(&dir)["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_string())
        .filter(|n| n.ends_with(&format!(" {stuck}")))
        .collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(telemetry::sender_status(&dir).unwrap().starts_with(&stuck));
    drop(lock);
    std::fs::remove_file(&owner).unwrap();
    let r = spool.flush_recorded_with(|_| true).unwrap();
    assert_eq!(r.sent, 1);
    assert!(
        !last_record(&dir)["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().contains("sender stuck since")),
        "{}",
        last_record(&dir)
    );
}
