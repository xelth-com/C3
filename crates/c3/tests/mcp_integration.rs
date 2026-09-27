//! Spawns the real `c3 mcp` stdio server and drives a short JSON-RPC session:
//! `initialize`, `tools/list`, and two read-only `tools/call`s. The `c3` binary
//! is built by the `c3-cli` crate, so it is located relative to the test binary
//! (`target/debug/deps/<test>` → `target/debug/c3[.exe]`).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

/// The path to the built `c3` executable, next to the test binary's target dir.
fn c3_bin() -> Option<PathBuf> {
    let mut dir = std::env::current_exe().ok()?;
    dir.pop(); // the test binary file
    if dir.ends_with("deps") {
        dir.pop();
    }
    let exe = dir.join(format!("c3{}", std::env::consts::EXE_SUFFIX));
    exe.exists().then_some(exe)
}

#[test]
fn mcp_stdio_session() {
    let Some(bin) = c3_bin() else {
        eprintln!("skipping: c3 binary not built at target/debug — run `cargo build` first");
        return;
    };

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
