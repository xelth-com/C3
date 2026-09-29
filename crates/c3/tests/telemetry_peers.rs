//! M11 privacy test for the telemetry `peers_used` field.
//!
//! Nothing federated ever reaches the maintainer's server: the consultation event carries the
//! COUNT of peers used, never a name, a path or a connection string. This mirrors the seeded-
//! secret allowlist test in `telemetry.rs` for the new integer field — it proves the field is
//! a plain number in the payload and that no poisoned text can ride alongside it.

use c3_core::ledger::{LedgerEntry, Reviewer};

use c3::telemetry::Event;

const POISON: &str = "LEAK /home/u/.ssh/id_rsa peer=hub conn=ws://user:sk-live-DEADBEEF@10.0.0.9";

fn poisoned_entry() -> LedgerEntry {
    LedgerEntry {
        n: 3,
        purpose: POISON.into(),
        brief: POISON.into(),
        reviewer: Reviewer {
            provider: POISON.into(),
            model: POISON.into(),
            engine: POISON.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn peers_used_is_a_plain_integer_and_leaks_nothing() {
    let entry = poisoned_entry();
    let mut event = Event::from_ledger(&entry, Some(2), "testinstance");
    // The consult path sets the peer count; a name/path/conn is never available to set.
    event.details.peers_used = 4;

    let value = serde_json::to_value(&event).unwrap();
    // The field is present and is an unsigned integer.
    assert_eq!(value["details"]["peers_used"], serde_json::json!(4));
    assert!(value["details"]["peers_used"].is_u64());

    // No poisoned text reaches the payload alongside it.
    let serialized = serde_json::to_string(&event).unwrap();
    for needle in [
        "LEAK", "id_rsa", "sk-live", "DEADBEEF", "10.0.0.9", "ws://", "hub",
    ] {
        assert!(
            !serialized.contains(needle),
            "the payload leaked `{needle}`: {serialized}"
        );
    }
}
