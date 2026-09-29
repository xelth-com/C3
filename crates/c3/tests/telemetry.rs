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

use c3::telemetry::{self, Config, Event, RatingEvent, Spool};

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
    // The rating event (M9 §7) is the later usefulness mark. Poison every text input,
    // including the topic tags, and prove none reaches the payload.
    let entry = poisoned_entry();
    const POISON: &str = "LEAK /home/u/.ssh/id_rsa secret=sk-live-DEADBEEF password token";
    let topics = vec![
        POISON.to_string(),
        "security".to_string(),
        "not-a-real-tag".to_string(),
    ];
    let event = RatingEvent::from_rating(&entry, "yes", 12, &topics, "testinstance");
    let serialized = serde_json::to_string(&event).unwrap();

    for needle in [
        "LEAK",
        "id_rsa",
        "sk-live",
        "DEADBEEF",
        "/home",
        "password",
        ".ssh",
        "secret=",
        "not-a-real-tag",
    ] {
        assert!(
            !serialized.contains(needle),
            "the rating payload leaked `{needle}`: {serialized}"
        );
    }
    let v: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    assert_eq!(v["event_type"], "rating");
    let d = &v["details"];
    assert_eq!(d["engine"], "other");
    assert_eq!(d["provider"], "unknown");
    assert_eq!(d["model"], "unknown");
    assert_eq!(d["purpose"], "unknown");
    assert_eq!(d["mark"], "yes");
    assert_eq!(d["age_days"], 12);
    // Only the one vocabulary word survives; the secret and the unknown tag are dropped.
    assert_eq!(d["topic_tags"], serde_json::json!(["security"]));
    assert_eq!(v["title"], "unknown");
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

#[test]
fn forget_me_deletes_local_state_and_asks_server() {
    let dir = temp_dir("forget");
    // Seed a salt, a spool and a stored reference.
    telemetry::instance_id_in(&dir);
    std::fs::write(dir.join("spool.ndjson"), "line\n").unwrap();
    std::fs::write(
        dir.join("refs.ndjson"),
        "{\"public_ref\":\"T-7KQ4-M2XZ\"}\n",
    )
    .unwrap();
    let (hub, _rx) = spawn_mock(200, "OK", r#"{"ok":true}"#);

    let outcome = telemetry::forget_me_at(&hub, &dir, |_plan| true).unwrap();
    assert!(outcome.confirmed);
    assert!(outcome.server_requested);
    assert!(outcome.server_deleted);
    assert!(!dir.join("salt").exists());
    assert!(!dir.join("spool.ndjson").exists());
    assert!(!dir.join("refs.ndjson").exists());
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
