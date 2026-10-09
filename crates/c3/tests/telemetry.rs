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
