//! Security test (wave 27, brief item 1 / KEEP #1): the child-environment scrub removes every
//! host marker from a launched child, and never leaks a VALUE. A test helper (a plain env dump,
//! NOT a reviewer) is launched through the same scrub the reviewer CLIs and probes use
//! (`c3::engines::scrub_host_markers`); every listed variable is set to a seeded marker and none of
//! the seeds reaches the child. The operator's own kept variables survive.
//!
//! The ledger/handoff/event-log/console never carry a value either: `child_env_scrubbed` stores the
//! NAMES only (`c3_core::host::host_marker_names` + the ledger field), covered by the field-order
//! and byte-identity tests; this test pins the child-process half — the security floor.

use std::process::Command;

/// Every scrubbed name to seed: the exact names plus one witness per wildcard prefix.
fn seeded_names() -> Vec<String> {
    let mut v: Vec<String> = c3_core::host::HOST_MARKER_NAMES
        .iter()
        .map(|s| s.to_string())
        .collect();
    v.push("CODEX_SANDBOX_NETWORK".to_string()); // CODEX_SANDBOX*
                                                 // (wave 27c, D21) the whole ZCODE_ prefix is scrubbed: the former exact names, the plugin
                                                 // roots, the provider-config / build / process names, and any name a later build adds.
    for z in [
        "ZCODE_PLUGIN_ROOT",
        "ZCODE_PLUGIN_DATA",
        "ZCODE_SESSION_ID",
        "ZCODE_PROJECT_DIR",
        "ZCODE_APP_VERSION",
        "ZCODE_BASE_URL",
        "ZCODE_PERSONAL_PROVIDER_CONFIG_FILE",
        "ZCODE_A_LATER_BUILD_ADDS_THIS",
    ] {
        v.push(z.to_string());
    }
    v
}

#[test]
fn scrub_removes_every_seeded_marker_and_keeps_the_operators_own() {
    // Seed a distinctive value for every marker, plus the kept variables.
    let seed = |name: &str| format!("SEED-{name}-27b");
    for name in seeded_names() {
        std::env::set_var(&name, seed(&name));
    }
    // KEPT: the operator's own settings and the plugin roots are NOT markers (exact names only,
    // never the whole CLAUDE_CODE_ prefix).
    std::env::set_var("CLAUDE_CODE_USE_BEDROCK", "KEEP-BEDROCK-27b");
    std::env::set_var("CLAUDE_PLUGIN_ROOT", "KEEP-ROOT-27b");
    std::env::set_var("CLAUDE_PLUGIN_DATA", "KEEP-DATA-27b");

    // Launch a plain env-dump helper (NOT a reviewer) through the reviewer scrub.
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", "set"]);
        c
    } else {
        Command::new("env")
    };
    c3::engines::scrub_host_markers(&mut cmd);
    let out = cmd.output().expect("the env-dump helper runs");
    let dump = String::from_utf8_lossy(&out.stdout).to_string();

    // No seeded marker value — nor its name with our seed — reaches the child.
    for name in seeded_names() {
        let s = seed(&name);
        assert!(
            !dump.contains(&s),
            "the child inherited the scrubbed marker {name} (value {s})"
        );
    }

    // The operator's own variables survive (exact-name scrub, not a prefix sweep).
    assert!(
        dump.contains("KEEP-BEDROCK-27b"),
        "CLAUDE_CODE_USE_BEDROCK must survive the scrub"
    );
    assert!(
        dump.contains("KEEP-ROOT-27b"),
        "CLAUDE_PLUGIN_ROOT must survive the scrub"
    );
    assert!(
        dump.contains("KEEP-DATA-27b"),
        "CLAUDE_PLUGIN_DATA must survive the scrub"
    );

    // `host_marker_names` reports the NAMES only (sorted), never a value — the ledger value.
    let names = c3_core::host::host_marker_names();
    for name in c3_core::host::HOST_MARKER_NAMES {
        assert!(
            names.iter().any(|n| n == name),
            "host_marker_names must list {name} when it is set"
        );
    }
    assert!(!names.iter().any(|n| n == "CLAUDE_CODE_USE_BEDROCK"));
    assert!(!names.iter().any(|n| n == "CLAUDE_PLUGIN_ROOT"));

    // Cleanup so no other test in this binary inherits the seeds.
    for name in seeded_names() {
        std::env::remove_var(&name);
    }
    std::env::remove_var("CLAUDE_CODE_USE_BEDROCK");
    std::env::remove_var("CLAUDE_PLUGIN_ROOT");
    std::env::remove_var("CLAUDE_PLUGIN_DATA");
}
