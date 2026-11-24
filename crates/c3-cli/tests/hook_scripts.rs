//! The plugin's SessionStart hook scripts (`plugin/hooks/c3-hook.ps1` and `c3-hook.sh`) forward,
//! as they are, the lines of `c3 hook` that start with `c3: `, `codex-consult: ` or `CI: ` - in the
//! order printed, each once, at most 10 - and then print the waiting rule. (0.2.5) `c3 hook` caps
//! its own lines (400 characters, `crates/c3/src/hook/mod.rs`), so the scripts cut nothing and the
//! two print identical text - non-ASCII lines included. A fake `c3` (named by `C3_EXE`) prints a
//! prepared file; the scripts run as a plugin runs them: the `.ps1` under Windows PowerShell 5.1
//! (Windows only), the `.sh` under `sh` (skipped with a message where there is none).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn scratch_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "c3-hookscripts-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn hooks_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("plugin")
        .join("hooks")
}

/// Forward slashes (an MSYS `sh` takes `C:/...` paths; Unix paths are unchanged).
fn slashes(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn sh_available() -> bool {
    Command::new("sh")
        .args(["-c", "command -v awk >/dev/null && command -v sed >/dev/null && command -v tr >/dev/null && command -v cut >/dev/null"])
        .stdin(Stdio::null())
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The fake `c3`: prints `out_file`'s bytes whatever it is asked.
fn fake_c3(dir: &Path, sh: bool) -> PathBuf {
    if sh {
        let p = dir.join("fake-c3.sh");
        std::fs::write(&p, "#!/bin/sh\ncat \"$FAKE_C3_OUT\"\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        p
    } else {
        let p = dir.join("fake-c3.cmd");
        std::fs::write(&p, "@echo off\r\ntype \"%FAKE_C3_OUT%\"\r\nexit /b 0\r\n").unwrap();
        p
    }
}

/// The `.ps1` under Windows PowerShell 5.1: stdout with `\n` line ends. `None` off Windows.
fn run_ps1(dir: &Path, output: &str) -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    let out_file = dir.join("c3-out.txt");
    std::fs::write(&out_file, output).unwrap();
    let fake = fake_c3(dir, false);
    let o = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(hooks_dir().join("c3-hook.ps1"))
        .env("C3_EXE", &fake)
        .env("FAKE_C3_OUT", &out_file)
        .env_remove("CLAUDE_PLUGIN_ROOT")
        .stdin(Stdio::null())
        .output()
        .expect("spawn powershell");
    assert!(o.status.success(), "the hook script must exit 0");
    Some(String::from_utf8_lossy(&o.stdout).replace("\r\n", "\n"))
}

/// The `.sh` under `sh`. `None` (with a message) where there is no `sh`.
fn run_sh(dir: &Path, output: &str) -> Option<String> {
    if !sh_available() {
        eprintln!("skipped: no `sh` (with awk, sed, tr, cut) on PATH");
        return None;
    }
    let out_file = dir.join("c3-out.txt");
    std::fs::write(&out_file, output).unwrap();
    let fake = fake_c3(dir, true);
    let o = Command::new("sh")
        .arg(slashes(&hooks_dir().join("c3-hook.sh")))
        .env("C3_EXE", slashes(&fake))
        .env("FAKE_C3_OUT", slashes(&out_file))
        .env_remove("CLAUDE_PLUGIN_ROOT")
        .stdin(Stdio::null())
        .output()
        .expect("spawn sh");
    assert!(o.status.success(), "the hook script must exit 0");
    Some(String::from_utf8_lossy(&o.stdout).replace("\r\n", "\n"))
}

/// Both scripts on the same `c3 hook` output; when both ran their texts are identical. Returns the
/// text of whichever ran (at least one must have).
fn forwarded(name: &str, output: &str) -> String {
    let dir = scratch_dir(name);
    let ps = run_ps1(&dir, output);
    let sh = run_sh(&dir, output);
    let _ = std::fs::remove_dir_all(&dir);
    if let (Some(a), Some(b)) = (&ps, &sh) {
        assert_eq!(a, b, "the two scripts printed different text");
    }
    ps.or(sh).expect("neither the .ps1 nor the .sh could run")
}

const RULE_PREFIX: &str = "c3: WAITING RULE (the operator's standing instruction";

/// The printed lines without the closing waiting rule (which is asserted to be last).
fn lines_before_rule(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let last = lines.pop().expect("a waiting rule line");
    assert!(last.starts_with(RULE_PREFIX), "last line: {last}");
    assert!(last.contains("delaySeconds 1800"), "{last}");
    lines
}

#[test]
fn the_scripts_forward_the_status_maintenance_branch_model_and_ci_lines() {
    // (0.2.5) a line longer than c3 hook would print is still forwarded whole: the cap is c3's
    let long = format!("c3: {}", "x".repeat(600));
    // a long non-ASCII line as c3 hook prints it (258 characters, 708 bytes; it ends in ASCII)
    let wide = format!(
        "c3: model settings refreshed - {}{} end",
        "\u{fc}".repeat(150),
        "\u{1F600}".repeat(100)
    );
    let avail = "codex-consult: 2 reviewer(s) available";
    let output = format!(
        "{avail}\n\
         codex-consult: coordinator rules - skill codex-consult:coordinate (or c3 consult --explain coordinate); telemetry: on\n\
         c3: maintenance pass finished - 1 update applied\n\
         c3: 2 branch(es) without a recorded state - c3 branches\n\
         c3: model settings refreshed in this repository - .claude/agents/a.md claude-opus-4-8 -> opus (left uncommitted)\n\
         c3: maintenance pass finished - 1 update applied\n\
         some noise without a known prefix\n\
         c3 hook debug: not forwarded either\n\
         {long}\n\
         {wide}\n\
         CI: the latest completed run on main failed (run 7) - gh run view 7\n"
    );
    let text = forwarded("set", &output);
    let lines = lines_before_rule(&text);
    assert_eq!(
        lines,
        vec![
            avail.to_string(),
            "codex-consult: coordinator rules - skill codex-consult:coordinate (or c3 consult --explain coordinate); telemetry: on".to_string(),
            "c3: maintenance pass finished - 1 update applied".to_string(),
            "c3: 2 branch(es) without a recorded state - c3 branches".to_string(),
            "c3: model settings refreshed in this repository - .claude/agents/a.md claude-opus-4-8 -> opus (left uncommitted)".to_string(),
            long,
            wide,
            "CI: the latest completed run on main failed (run 7) - gh run view 7".to_string(),
        ],
        "{text}"
    );
    assert!(!text.contains("noise"), "noise was forwarded");
}

#[test]
fn the_scripts_forward_at_most_ten_lines() {
    let output: String = (1..=14).map(|i| format!("c3: line {i}\n")).collect();
    let text = forwarded("ten", &output);
    let lines = lines_before_rule(&text);
    let want: Vec<String> = (1..=10).map(|i| format!("c3: line {i}")).collect();
    assert_eq!(lines, want, "{text}");
}

#[test]
fn the_scripts_keep_their_fallbacks() {
    // no output at all
    let text = forwarded("empty", "");
    assert_eq!(
        lines_before_rule(&text),
        vec!["c3 hook exited 0 without output".to_string()],
        "{text}"
    );
    // output without any known prefix: its first non-empty line, as it is
    let text = forwarded("noise", "\nerror: something odd\nmore\n");
    assert_eq!(
        lines_before_rule(&text),
        vec!["error: something odd".to_string()],
        "{text}"
    );
    // (F45-4) ...cut at 400 characters by the scripts themselves: a panic or library line that
    // bypassed `c3 hook`'s own cap does not reach the session whole
    let text = forwarded("long-noise", &format!("\n{}\nmore\n", "e".repeat(5000)));
    assert_eq!(lines_before_rule(&text), vec!["e".repeat(400)], "{text}");
}

/// The `.ps1` under Windows PowerShell 5.1 with a fake `c3` that is itself a PowerShell script
/// (`body`): the hook script runs it in-process, so its strings reach the hook as .NET strings (an
/// emoji stays one surrogate pair) and a `throw` in it is an exception inside the hook's `try`.
/// `None` off Windows.
fn run_ps1_script(name: &str, body: &str) -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    let dir = scratch_dir(name);
    let fake = dir.join("fake-c3.ps1");
    std::fs::write(&fake, body).unwrap();
    let o = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(hooks_dir().join("c3-hook.ps1"))
        .env("C3_EXE", &fake)
        .env_remove("CLAUDE_PLUGIN_ROOT")
        .stdin(Stdio::null())
        .output()
        .expect("spawn powershell");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(o.status.success(), "the hook script must exit 0");
    Some(String::from_utf8_lossy(&o.stdout).replace("\r\n", "\n"))
}

// (0.2.5; R4, F16-10 / F17-9) The `.ps1` no longer cuts a line - so it can never split a surrogate
// pair: an emoji at UTF-16 units 399-400 (where the old cut fell) is forwarded with the rest of the
// line on the forwarded path; the no-prefix fallback (cut at 400 again, F45-4) stops before it.
#[test]
fn the_ps1_forwards_a_long_line_whole() {
    let Some(text) = run_ps1_script(
        "surrogate",
        "$e = [char]::ConvertFromUtf32(0x1F600)\r\nWrite-Output ('c3: ' + ('x' * 395) + $e + 'tail')\r\n",
    ) else {
        eprintln!("skipped: the .ps1 runs on Windows only");
        return;
    };
    let lines = lines_before_rule(&text);
    assert_eq!(lines.len(), 1, "{text}");
    assert!(
        lines[0].starts_with(&format!("c3: {}", "x".repeat(395))) && lines[0].ends_with("tail"),
        "{text}"
    );
    assert!(!lines[0].ends_with("..."), "{text}");
    let text = run_ps1_script(
        "surrogate-first",
        "$e = [char]::ConvertFromUtf32(0x1F600)\r\nWrite-Output (('y' * 399) + $e + 'z')\r\n",
    )
    .unwrap();
    let lines = lines_before_rule(&text);
    // (F45-4) the fallback is cut at 400 again - before the surrogate pair, never inside it
    assert_eq!(lines, vec!["y".repeat(399)], "{text}");
}

// (R4, F17-9) An exception inside the `.ps1` still ends with the waiting rule, exactly as the normal
// path prints it (the `.sh` always prints it).
#[test]
fn the_ps1_catch_path_prints_the_waiting_rule() {
    let Some(text) = run_ps1_script("catch", "throw 'boom from the fake c3'\r\n") else {
        eprintln!("skipped: the .ps1 runs on Windows only");
        return;
    };
    assert_eq!(
        lines_before_rule(&text),
        vec!["c3: hook check failed - boom from the fake c3".to_string()],
        "{text}"
    );
    let normal = forwarded("catch-normal", "c3: fine\n");
    assert_eq!(
        text.lines().last(),
        normal.lines().last(),
        "the catch path's rule differs from the normal path's"
    );
}
