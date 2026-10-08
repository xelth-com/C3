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
    assert_eq!(d["engine"], "other");
    assert_eq!(d["provider"], "unknown");
    assert_eq!(d["model"], "unknown");
    assert_eq!(d["purpose"], "unknown");
    assert_eq!(d["outcome"], "failed:unknown");
    // The numeric/boolean allowlist still carries the useful shape.
    assert_eq!(d["tokens_in"], 12000);
    assert_eq!(d["tokens_out"], 900);
    assert_eq!(d["findings"], 4);
    assert_eq!(d["structured"], true);
    assert_eq!(d["format_retry"], true);
    assert_eq!(d["panel_size"], 3);
    // The title mirrors the safe purpose label, never the raw text.
    assert_eq!(v["title"], "unknown");
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
            model: "gpt-5".into(),
            engine: "codex".into(),
            ..Default::default()
        },
        bridge_outcome: "ok".into(),
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

fn enqueue_one(spool: &Spool, purpose: &str) {
    let entry = LedgerEntry {
        purpose: purpose.into(),
        reviewer: Reviewer {
            engine: "codex".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let event = Event::from_ledger(&entry, Some(1), "testinstance");
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
    let old = serde_json::json!({"app_id": "c3", "event_type": "rating", "client_time": "2000-01-01T00:00:00Z", "details": {}});
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
        r#"{{"app_id":"c3","event_type":"consultation","client_time":"{now}","details":{{}}}}"#
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
