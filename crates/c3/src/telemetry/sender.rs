//! (wave 6) The detached sender (`Start-TelemetrySender`, the plugin's wave 28 R17 / 28b D3).
//!
//! Right after a commit that spooled an event - a consultation's, a panel's (once, for every
//! member), a rating's, a backfill's - the run starts ONE detached process, `c3 telemetry --flush
//! --telemetry on`, which flushes the outbox once ([`super::flush_now`]: the sender lock, its owner
//! record, the last flush's record) and exits. The run never waits for it, so a session's last
//! event does not wait for the next run; a sender that finds the lock busy skips, as every flush
//! does.
//!
//! The process holds NONE of the run's handles (Windows: `CreateProcessW` with `bInheritHandles =
//! FALSE`, `CREATE_NO_WINDOW`; elsewhere its standard streams are null and it runs in its own
//! process group), starts in the temporary directory, and gets an ALLOW-LISTED environment
//! ([`sender_environment`]) - never a provider key, a host marker or another `CODEX_CONSULT_*`
//! variable.

use std::path::{Path, PathBuf};

/// The variables a sender may inherit (`$script:TelemetrySenderEnvNames`), compared ignoring case.
const SENDER_ENV_NAMES: &[&str] = &[
    "SystemRoot",
    "windir",
    "SystemDrive",
    "ComSpec",
    "PATH",
    "PATHEXT",
    "TEMP",
    "TMP",
    "TMPDIR",
    "USERPROFILE",
    "HOME",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    "ProgramData",
    "ALLUSERSPROFILE",
    "PSModulePath",
    "PROCESSOR_ARCHITECTURE",
    "NUMBER_OF_PROCESSORS",
    "OS",
    "LANG",
    "LANGUAGE",
    "TZ",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "ALL_PROXY",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "REQUESTS_CA_BUNDLE",
    "CURL_CA_BUNDLE",
    "NODE_EXTRA_CA_CERTS",
    "POWERSHELL_TELEMETRY_OPTOUT",
    "POWERSHELL_UPDATECHECK",
    "CODEX_HOME",
    "CODEX_CONSULT_TELEMETRY",
    "CODEX_CONSULT_TELEMETRY_URL",
    // C3's own intake override (`telemetry::hub`): the sender posts where the run would have
    "C3_TELEMETRY_HUB",
];

/// The prefixes a sender's variable may start with (`$script:TelemetrySenderEnvPrefixes`).
const SENDER_ENV_PREFIXES: &[&str] = &[
    "ProgramFiles",
    "CommonProgramFiles",
    "ProgramW6432",
    "CommonProgramW6432",
    "LC_",
];

/// The sender's own test hooks travel under this prefix (test mode only), as the plugin's do.
const SENDER_TEST_PREFIX: &str = "CODEX_CONSULT_TEST_TELEMETRY_";

/// TEST HOOK (test mode only): C3's plugin-home hook ([`super::notspooled::PLUGIN_HOME_VAR`]) as
/// the sender receives it - under the sender-hook prefix, the one test name the allow list admits.
pub const SENDER_PLUGIN_HOME_VAR: &str = "CODEX_CONSULT_TEST_TELEMETRY_PLUGIN_HOME";

/// `Test-TelemetrySenderEnvName`: whether a variable may reach the sender - the names and prefixes
/// above (any case); in test mode also `CODEX_CONSULT_TEST_MODE` and
/// `CODEX_CONSULT_TEST_TELEMETRY_*`.
pub fn is_sender_env_name(name: &str, test_mode: bool) -> bool {
    let u = name.to_ascii_uppercase();
    if SENDER_ENV_NAMES.iter().any(|n| n.to_ascii_uppercase() == u) {
        return true;
    }
    if SENDER_ENV_PREFIXES
        .iter()
        .any(|p| u.starts_with(&p.to_ascii_uppercase()))
    {
        return true;
    }
    test_mode && (u == c3_core::test_hooks::MODE_VAR || u.starts_with(SENDER_TEST_PREFIX))
}

/// `Get-TelemetrySenderEnvironment` over `vars` (this process's environment): the allowed
/// variables, sorted by name ignoring case (Windows: one per name ignoring case), plus `CODEX_HOME`
/// named even when it was derived (the sender flushes the outbox THIS run wrote), plus - test mode
/// - the plugin-home hook as [`SENDER_PLUGIN_HOME_VAR`].
pub fn sender_environment_from(
    vars: impl IntoIterator<Item = (String, String)>,
    test_mode: bool,
    codex_home: &str,
) -> Vec<(String, String)> {
    let same = |a: &str, b: &str| {
        if cfg!(windows) {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };
    let mut out: Vec<(String, String)> = Vec::new();
    let mut plugin_home = String::new();
    for (k, v) in vars {
        if k.is_empty() || out.iter().any(|(n, _)| same(n, &k)) {
            continue;
        }
        if test_mode && k == super::notspooled::PLUGIN_HOME_VAR {
            plugin_home = v.clone();
        }
        if is_sender_env_name(&k, test_mode) {
            out.push((k, v));
        }
    }
    if !out.iter().any(|(n, _)| same(n, "CODEX_HOME")) && !codex_home.is_empty() {
        out.push(("CODEX_HOME".into(), codex_home.to_string()));
    }
    if test_mode
        && !plugin_home.is_empty()
        && !out.iter().any(|(n, _)| same(n, SENDER_PLUGIN_HOME_VAR))
    {
        out.push((SENDER_PLUGIN_HOME_VAR.into(), plugin_home));
    }
    out.sort_by(|a, b| {
        a.0.to_ascii_uppercase()
            .cmp(&b.0.to_ascii_uppercase())
            .then_with(|| a.0.cmp(&b.0))
    });
    out
}

/// The sender's environment from THIS process's (read only: this process's is never changed).
pub fn sender_environment() -> Vec<(String, String)> {
    // the codex home is the telemetry directory's grandparent (`<codex home>/c3/telemetry`)
    let home = super::telemetry_dir()
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    sender_environment_from(
        std::env::vars_os()
            .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?))),
        c3_core::test_hooks::mode_on(),
        &home,
    )
}

/// Start the detached sender (not waited for): `Ok` when it started, else why not. Never blocks
/// the caller beyond the process creation.
pub fn start_sender() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("the c3 executable is unknown ({e})"))?;
    let env = sender_environment();
    spawn(&exe, &env, &std::env::temp_dir())
}

/// [`start_sender`] as a run calls it: silent when it started; the reason under `C3_DEBUG` when
/// not (the plugin's `Write-Verbose`) - a run never fails over its sender.
pub fn start_sender_quietly() {
    if let Err(why) = start_sender() {
        super::debug_log(&format!("the sender did not start ({why})"));
    }
}

#[cfg(windows)]
fn spawn(exe: &Path, env: &[(String, String)], cwd: &Path) -> Result<(), String> {
    let program = exe.to_string_lossy().to_string();
    // SAFETY: a straight CreateProcessW with bInheritHandles=FALSE (the helper's contract); every
    // buffer it is given outlives the call.
    unsafe {
        crate::consult::detach::create_process_no_inherit(
            &program,
            "telemetry --flush --telemetry on",
            cwd,
            Some(env),
        )
    }
    .map_err(|e| e.replace("the background process", "the telemetry sender"))
}

#[cfg(not(windows))]
fn spawn(exe: &Path, env: &[(String, String)], cwd: &Path) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(exe);
    cmd.args(["telemetry", "--flush", "--telemetry", "on"])
        .env_clear()
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not start the telemetry sender ({e})"))?;
    // reaped by a detached thread (a long-lived caller - the MCP server - keeps no zombie)
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// TEST HOOK (test mode only, `CODEX_CONSULT_TEST_TELEMETRY_ENV=<path>`): a sender writes there
/// the NAMES of every variable of its own environment (never a value), sorted ignoring case, one
/// per line - the plugin's `codex-telemetry.ps1 -Flush` hook.
pub fn dump_env_names_hook() {
    let Some(path) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_TELEMETRY_ENV") else {
        return;
    };
    if path.trim().is_empty() {
        return;
    }
    let mut names: Vec<String> = std::env::vars_os()
        .filter_map(|(k, _)| k.into_string().ok())
        .collect();
    names.sort_by_key(|a| a.to_ascii_uppercase());
    let mut text = names.join("\n");
    if !names.is_empty() {
        text.push('\n');
    }
    let _ = std::fs::write(PathBuf::from(path), text);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn the_allow_list_is_the_plugins() {
        for n in [
            "Path",
            "SYSTEMROOT",
            "https_proxy",
            "ProgramFiles(x86)",
            "LC_ALL",
            "CODEX_HOME",
        ] {
            assert!(is_sender_env_name(n, false), "{n}");
        }
        for n in [
            "CLAUDECODE",
            "CODEX_THREAD_ID",
            "ZCODE_SESSION_ID",
            "RT_ZAI_KEY",
            "CODEX_CONSULT_ROSTER",
            "CODEX_CONSULT_TEST_MODE",
            "CODEX_CONSULT_TEST_TELEMETRY_ENV",
            "C3_PRIORS",
        ] {
            assert!(!is_sender_env_name(n, false), "{n}");
        }
        assert!(is_sender_env_name("CODEX_CONSULT_TEST_MODE", true));
        assert!(is_sender_env_name(
            "CODEX_CONSULT_TEST_TELEMETRY_FLUSH_MS",
            true
        ));
        assert!(!is_sender_env_name(
            "CODEX_CONSULT_TEST_REGISTER_FAIL",
            true
        ));
        assert!(!is_sender_env_name("C3_TEST_TELEMETRY_PLUGIN_HOME", true));
    }

    #[test]
    fn the_environment_names_the_codex_home_and_carries_the_plugin_home_hook() {
        let vars = v(&[
            ("PATH", "p"),
            ("CLAUDECODE", "1"),
            ("RT_ZAI_KEY", "secret"),
            ("C3_TEST_TELEMETRY_PLUGIN_HOME", "/h"),
            ("CODEX_CONSULT_TEST_MODE", "1"),
            ("C3_TELEMETRY_HUB", "http://127.0.0.1:9/T"),
        ]);
        let env = sender_environment_from(vars.clone(), true, "/codex");
        let names: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "C3_TELEMETRY_HUB",
                "CODEX_CONSULT_TEST_MODE",
                SENDER_PLUGIN_HOME_VAR,
                "CODEX_HOME",
                "PATH"
            ]
        );
        assert!(env
            .iter()
            .any(|(k, v)| k == SENDER_PLUGIN_HOME_VAR && v == "/h"));
        assert!(env.iter().any(|(k, v)| k == "CODEX_HOME" && v == "/codex"));
        // outside test mode: no test variable at all
        let env = sender_environment_from(vars, false, "/codex");
        assert!(env.iter().all(|(k, _)| !k.contains("TEST")), "{env:?}");
        // a CODEX_HOME the run has is kept as it is
        let env = sender_environment_from(v(&[("CODEX_HOME", "/mine")]), false, "/codex");
        assert_eq!(env, v(&[("CODEX_HOME", "/mine")]));
    }
}
