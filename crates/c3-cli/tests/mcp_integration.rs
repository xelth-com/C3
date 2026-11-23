//! Spawns the real `c3 mcp` stdio server and drives a short JSON-RPC session:
//! `initialize`, `tools/list`, and two read-only `tools/call`s.
//!
//! This test lives in `c3-cli` (the crate that builds the `c3` binary) so cargo builds the binary
//! with the test's own feature set and hands its path to the test through `CARGO_BIN_EXE_c3`. The
//! test therefore never depends on whatever stale `c3` happens to lie in `target/debug`.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

/// The `c3` executable cargo built for this test (never a stale on-disk binary).
fn c3_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_c3"))
}

/// (0.2.5 R5, F19-5) Hostile `rev` / `diff` values sent through the MCP tool are refused by the
/// tool itself - with the CLI's own refusal text, before any `c3 consult` (and so any git process)
/// starts: a value that would be a git option (`--output=<file>` would create that file), `-c`, a
/// value with a line break or a control character.
#[test]
fn mcp_consult_refuses_hostile_rev_and_diff_before_anything_starts() {
    let work = std::env::temp_dir().join(format!(
        "c3-mcp-hostile-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&work).unwrap();
    let marker = work.join("git-option-honoured.txt");
    let marker_s = marker.to_string_lossy().to_string();
    let mut child = Command::new(c3_bin())
        .arg("mcp")
        .current_dir(&work)
        .env("CODEX_CONSULT_TELEMETRY", "off")
        .env("C3_PRIORS", "off")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn c3 mcp");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut call = |id: i64, key: &str, val: &str| -> Value {
        let mut arguments = json!({
            "task": "t", "purpose": "framing", "brief": "brief.md", "prompt": "p",
            "reply_name": "r" });
        arguments[key] = Value::String(val.to_string());
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": "c3_consult", "arguments": arguments } });
        stdin.write_all(msg.to_string().as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        serde_json::from_str(line.trim()).unwrap_or_else(|e| panic!("bad frame {line:?}: {e}"))
    };
    let output_opt = format!("--output={marker_s}");
    let cases: Vec<(&str, &str, &str)> = vec![
        ("rev", output_opt.as_str(), "-Rev '"),
        ("rev", "-c", "-Rev '"),
        ("rev", "HEAD\nx", "-Rev '"),
        ("rev", "HEAD\u{1}x", "-Rev '"),
        ("diff", output_opt.as_str(), "-Diff '"),
        ("diff", "-c", "-Diff '"),
        ("diff", "HEAD~1..HEAD\nx", "-Diff '"),
    ];
    for (i, (key, val, lead)) in cases.iter().enumerate() {
        let r = call(10 + i as i64, key, val);
        assert_eq!(r["result"]["isError"], true, "{key}={val:?}: {r}");
        let text = r["result"]["content"][0]["text"].as_str().unwrap_or("");
        assert!(
            text.starts_with(lead) && text.contains("must be"),
            "{key}={val:?} was not refused by the tool itself: {text}"
        );
    }
    drop(stdin);
    let _ = child.wait();
    assert!(!marker.exists(), "a value reached git as an option");
    assert!(
        !work.join(".collab").exists(),
        "a consultation was started for a refused value"
    );
    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn mcp_stdio_session() {
    let bin = c3_bin();

    let mut child = Command::new(&bin)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn c3 mcp");

    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    let send = |w: &mut std::process::ChildStdin, v: &Value| {
        w.write_all(v.to_string().as_bytes()).unwrap();
        w.write_all(b"\n").unwrap();
        w.flush().unwrap();
    };
    let recv = |r: &mut BufReader<std::process::ChildStdout>| -> Value {
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        serde_json::from_str(line.trim()).unwrap_or_else(|e| panic!("bad frame {line:?}: {e}"))
    };

    // initialize
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "protocolVersion": "2025-06-18", "capabilities": {} } }),
    );
    let init = recv(&mut reader);
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "c3");

    // notifications/initialized (no reply expected)
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    );

    // tools/list
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    );
    let list = recv(&mut reader);
    assert_eq!(list["id"], 2);
    let names: Vec<String> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"c3_telemetry_status".to_string()));
    assert!(names.contains(&"c3_providers".to_string()));
    assert!(names.contains(&"c3_consult".to_string()));
    assert!(names.contains(&"c3_panel".to_string()));
    assert!(names.contains(&"c3_status".to_string()));
    assert!(names.contains(&"c3_router_explain".to_string()));
    assert!(names.contains(&"c3_router_replay".to_string()));

    // tools/call c3_telemetry_status — always works, sends no network.
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "c3_telemetry_status", "arguments": {} } }),
    );
    let tel = recv(&mut reader);
    assert_eq!(tel["id"], 3);
    let text = tel["result"]["content"][0]["text"].as_str().unwrap();
    assert!(!text.is_empty(), "telemetry status produced no text");

    // tools/call c3_providers {short:true, no_network:true}. This shells out to
    // the codex launcher; if codex is not on PATH (CI) the tool reports an error
    // through isError rather than crashing — either outcome is acceptable here.
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": { "name": "c3_providers", "arguments": { "short": true, "no_network": true } } }),
    );
    let prov = recv(&mut reader);
    assert_eq!(prov["id"], 4);
    assert!(prov["result"]["content"][0]["text"].is_string());
    assert!(prov["result"]["isError"].is_boolean());

    // ping
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 5, "method": "ping" }),
    );
    let pong = recv(&mut reader);
    assert_eq!(pong["id"], 5);
    assert!(pong["result"].is_object());

    // unknown method → -32601
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 6, "method": "no/such" }),
    );
    let err = recv(&mut reader);
    assert_eq!(err["error"]["code"], -32601);

    drop(stdin); // closing stdin ends the serve loop
    let _ = child.wait();
}
