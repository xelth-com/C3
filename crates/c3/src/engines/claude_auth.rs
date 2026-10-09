//! The `claude` engine's credential side (plugin 0.6.0, wave 29 / 29b): THE child environment of
//! every claude process (an ALLOW list; D2/D3), the launcher probes run in it
//! (`Invoke-ClaudeProbe`: `claude auth status`, `claude --version`), the sign-in check
//! (`Get-ClaudeSignIn`, its facts cached per launcher and auth - `$script:ClaudeSignInCache`), the
//! local check no ledger evidence replaces (`Test-ClaudeLocalCredential`), the engine's credential
//! for the preflight and the listing (`Get-EngineCredential`), the harness string
//! (`Get-ClaudeHarness`) and the launch check (`Get-ClaudeLaunchProblem`). The turn itself is
//! [`super::claude`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use c3_core::claude::{ChildEnv, ClaudeEndpoint, CLAUDE_AUTH_MODES};
use c3_core::credential::CredentialResult;

fn one_line(s: &str) -> String {
    c3_core::one_line(s)
}

fn re(pat: &str) -> regex::Regex {
    regex::Regex::new(pat).expect("a valid built-in regex")
}

// --------------------------------------------------------------------------- the child environment

/// THE child environment of a claude process for `auth` (this process's environment, the test
/// hook `CODEX_CONSULT_TEST_CHILD_ENV_PASS` in test mode, the endpoint's token read NOW).
pub fn child_env(auth: &str, endpoint: Option<&ClaudeEndpoint>) -> ChildEnv {
    let vars: Vec<(String, String)> = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .collect();
    let pass = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_CHILD_ENV_PASS")
        .map(|v| c3_core::claude::valid_pass_prefix(&v))
        .unwrap_or_default();
    let token = if auth == "endpoint" {
        endpoint
            .and_then(|ep| std::env::var(&ep.env_key).ok())
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_default()
    } else {
        String::new()
    };
    c3_core::claude::child_environment(&vars, auth, endpoint, &pass, cfg!(windows), &token)
}

/// A launcher probe's result (`Invoke-ClaudeProbe`).
#[derive(Debug, Clone, Default)]
pub struct Probe {
    pub started: bool,
    pub why: String,
    pub exit: i32,
    pub timed_out: bool,
    pub out: String,
    pub err: String,
}

/// `Invoke-ClaudeProbe`: `<launcher> <args>` in the child environment of `auth` (D3), nothing on
/// stdin, stdout and stderr as UTF-8, killed (its tree) after `timeout_sec`.
pub fn probe(launcher: &str, args: &[&str], timeout_sec: u64, env: &ChildEnv) -> Probe {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut p = Probe {
        exit: -1,
        ..Default::default()
    };
    let mut cmd = Command::new(launcher);
    cmd.args(args)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in &env.env {
        cmd.env(k, v);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            p.why = one_line(&e.to_string());
            return p;
        }
    };
    p.started = true;
    let mut out = child.stdout.take().expect("piped");
    let mut err = child.stderr.take().expect("piped");
    let oh = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = out.read_to_end(&mut b);
        String::from_utf8_lossy(&b).to_string()
    });
    let eh = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = err.read_to_end(&mut b);
        String::from_utf8_lossy(&b).to_string()
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                p.exit = status.code().unwrap_or(-1);
                p.out = oh.join().unwrap_or_default();
                p.err = eh.join().unwrap_or_default();
                return p;
            }
            Ok(None) => {
                if start.elapsed() >= Duration::from_secs(timeout_sec) {
                    super::subprocess::kill_tree_by_pid(child.id());
                    let _ = child.kill();
                    let _ = child.wait();
                    p.timed_out = true;
                    return p;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                p.why = one_line(&e.to_string());
                return p;
            }
        }
    }
}

/// What `claude auth status` reported in this process (`$script:ClaudeSignInCache`): only
/// authMethod, apiProvider and projectsDirectory - never the account's e-mail or organisation.
#[derive(Debug, Clone, Default)]
pub struct SignInInfo {
    pub auth_method: String,
    pub api_provider: String,
    pub projects_directory: String,
}

fn sign_in_cache() -> &'static Mutex<HashMap<String, (SignInInfo, CredentialResult)>> {
    static C: OnceLock<Mutex<HashMap<String, (SignInInfo, CredentialResult)>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The cached `claude auth status` facts of `launcher|auth` (`None` when it did not run).
pub fn sign_in_info(launcher: &str, auth: &str) -> Option<SignInInfo> {
    sign_in_cache()
        .lock()
        .ok()?
        .get(&format!("{launcher}|{auth}"))
        .map(|(i, _)| i.clone())
}

/// `Test-ClaudeLocalCredential`: the local part of a claude entry's sign-in that no ledger evidence
/// replaces - auth api-key: `ANTHROPIC_API_KEY` set now; auth endpoint: a usable endpoint and its
/// token variable set now. `""` or the reason (a name, never a value).
pub fn local_credential(auth: &str, endpoint: Option<&ClaudeEndpoint>) -> String {
    if auth == "endpoint" {
        let ep = c3_core::claude::endpoint_problem(endpoint);
        if !ep.is_empty() {
            return ep;
        }
        let e = endpoint.expect("checked");
        let set = std::env::var(&e.env_key)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        if !set {
            return format!("env {} not set", e.env_key);
        }
        return String::new();
    }
    if auth != "api-key" {
        return String::new();
    }
    let set = std::env::var("ANTHROPIC_API_KEY")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    if set {
        String::new()
    } else {
        "ANTHROPIC_API_KEY is not set (roster auth api-key)".to_string()
    }
}

/// `Get-ClaudeSignIn`: `claude auth status` (local, free) in the child environment of `auth`,
/// its JSON read BEFORE its exit code; auth endpoint: no `claude auth status` (E3: it reads the
/// local login and ignores the base URL) - the endpoint, its token variable and (wave 4f, F25-1)
/// the launcher's `--version` probe ([`launcher_problem`]), then `ok: env <NAME> set`. One probe
/// per `launcher|auth` per process.
pub fn sign_in(
    launcher: &str,
    timeout_sec: u64,
    auth: &str,
    endpoint: Option<&ClaudeEndpoint>,
) -> CredentialResult {
    let auth = if CLAUDE_AUTH_MODES.contains(&auth) {
        auth
    } else {
        "subscription"
    };
    if launcher.is_empty() {
        return CredentialResult::missing("claude CLI not found on PATH");
    }
    if auth == "endpoint" {
        let ep = c3_core::claude::endpoint_problem(endpoint);
        if !ep.is_empty() {
            return CredentialResult::missing(ep);
        }
        let e = endpoint.expect("checked");
        let set = std::env::var(&e.env_key)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        if !set {
            return CredentialResult::missing(format!("env {} not set", e.env_key));
        }
        if let Some(r) = launcher_problem(launcher) {
            return r;
        }
        return CredentialResult::ok(format!("env {} set", e.env_key));
    }
    if auth == "api-key" {
        let w = local_credential(auth, endpoint);
        if !w.is_empty() {
            return CredentialResult::missing(w);
        }
    }
    let key = format!("{launcher}|{auth}");
    if let Some((_, r)) = sign_in_cache()
        .lock()
        .ok()
        .and_then(|c| c.get(&key).cloned())
    {
        return r;
    }
    let t = if timeout_sec > 0 {
        timeout_sec.min(15)
    } else {
        15
    };
    let env = child_env(auth, None);
    let cap = probe(launcher, &["auth", "status"], t, &env);
    if !cap.started {
        return CredentialResult::unknown(format!(
            "not checked - `claude auth status` could not be started{}",
            if cap.why.is_empty() {
                String::new()
            } else {
                format!(" ({})", cap.why)
            }
        ));
    }
    if cap.timed_out {
        return CredentialResult::unknown(format!(
            "not checked - `claude auth status` did not finish within {t} s"
        ));
    }
    let out = cap.out.clone();
    let obj: Option<Value> = match (out.find('{'), out.rfind('}')) {
        (Some(a), Some(b)) if b > a => serde_json::from_str(&out[a..=b]).ok(),
        _ => None,
    };
    let Some(o) = obj.filter(|v| v.is_object()) else {
        return CredentialResult::unknown(format!(
            "not checked - `claude auth status` printed no JSON object (exit {})",
            cap.exit
        ));
    };
    let li = o.get("loggedIn").and_then(|v| v.as_bool());
    let am = c3_core::claude::token(o.get("authMethod"));
    let ap = c3_core::claude::token(o.get("apiProvider"));
    let pd = o
        .get("projectsDirectory")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let info = SignInInfo {
        auth_method: am.clone(),
        api_provider: ap.clone(),
        projects_directory: pd,
    };
    let result = if li == Some(false) {
        CredentialResult::missing(
            "not signed in (`claude auth status`: loggedIn false; run `claude auth login`)",
        )
    } else if li != Some(true) {
        CredentialResult::unknown(format!(
            "not checked - `claude auth status` names no loggedIn (exit {})",
            cap.exit
        ))
    } else if !ap.is_empty() && ap != "firstParty" {
        CredentialResult::missing(format!("apiProvider {ap} - routes other than Anthropic's own API (a gateway, Bedrock, Vertex, Foundry) are out of scope for the claude engine"))
    } else if auth == "subscription" {
        if am != "claude.ai" {
            CredentialResult::missing(format!("signed in with authMethod {}, not the claude.ai subscription the roster names (auth subscription; an API key is auth api-key)", if am.is_empty() { "(none)" } else { &am }))
        } else {
            CredentialResult::ok("signed in (claude.ai subscription)")
        }
    } else {
        CredentialResult::ok(format!(
            "signed in (ANTHROPIC_API_KEY set{})",
            if am.is_empty() {
                String::new()
            } else {
                format!("; authMethod {am}")
            }
        ))
    };
    if let Ok(mut c) = sign_in_cache().lock() {
        c.insert(key, (info, result.clone()));
    }
    result
}

/// `Get-EngineCredential` for the claude engine (the preflight and the listing): no launcher ->
/// missing; the LOCAL check that no ledger evidence replaces (auth api-key: `ANTHROPIC_API_KEY`
/// set now; auth endpoint: the endpoint and its token variable); auth endpoint is checked every
/// time (`ok: env <NAME> set` - no `claude auth status`; wave 4f, F25-1: the launcher must answer
/// `--version`, except under `-NoNetwork`, which starts nothing); then a usable reply of this
/// endpoint within 60 minutes evidences the sign-in; `-NoNetwork` (the SessionStart hook) starts
/// nothing; else `claude auth status` (`CODEX_CONSULT_TEST_LOGIN_TIMEOUT` shortens the 15 s in
/// test mode).
pub fn engine_credential(
    launcher: &str,
    auth: &str,
    endpoint: Option<&ClaudeEndpoint>,
    health: Option<&c3_core::health::EndpointHealth>,
    no_network: bool,
) -> CredentialResult {
    let auth = if CLAUDE_AUTH_MODES.contains(&auth) {
        auth
    } else {
        "subscription"
    };
    if launcher.is_empty() {
        return CredentialResult::missing("claude CLI not found on PATH");
    }
    let local = local_credential(auth, endpoint);
    if !local.is_empty() {
        return CredentialResult::missing(local);
    }
    if auth == "endpoint" {
        if no_network {
            // the SessionStart hook starts no process: the local check above is the whole answer
            let e = endpoint.expect("checked by the local check");
            return CredentialResult::ok(format!("env {} set", e.env_key));
        }
        return sign_in(launcher, 0, auth, endpoint);
    }
    if let Some(ru) = health.and_then(|h| h.recent_usable.as_ref()) {
        return CredentialResult::ok(format!(
            "signed in (usable reply {} min ago)",
            ru.age_minutes
        ));
    }
    if no_network {
        return CredentialResult::with_detail(
            c3_core::credential::State::Unknown,
            "sign-in not checked",
            "not checked (launcher present; run codex-providers.ps1)",
        );
    }
    let mut timeout = 45u64;
    if let Some(hook) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_LOGIN_TIMEOUT") {
        if let Ok(n) = hook.trim().parse::<u64>() {
            if n > 0 {
                timeout = n;
            }
        }
    }
    sign_in(launcher, timeout, auth, None)
}

/// `Get-ClaudeLaunchProblem` (item 2): where the engine keeps its transcripts must not lie in the
/// repository under review - `CLAUDE_CONFIG_DIR`, and the projects directory
/// ([`projects_directory`]: the one `claude auth status` reported, else - wave 4f, F25-2 - the one
/// Claude Code derives, so auth endpoint, which runs no `claude auth status`, is checked too).
pub fn launch_problem(repo_root: &Path, launcher: &str, auth: &str) -> String {
    let cfg = std::env::var("CLAUDE_CONFIG_DIR").unwrap_or_default();
    let reported = sign_in_info(launcher, auth)
        .map(|i| i.projects_directory)
        .unwrap_or_default();
    launch_problem_of(repo_root, &cfg, &home_dir(), &reported)
}

/// [`launch_problem`] over its inputs: the value of `CLAUDE_CONFIG_DIR` (`""` = unset), the user's
/// home directory and the projects directory `claude auth status` reported (`""` = none). The
/// plugin's two refusals, verbatim.
pub fn launch_problem_of(repo_root: &Path, config_dir: &str, home: &str, reported: &str) -> String {
    let cfg = config_dir.trim();
    if !cfg.is_empty() && path_inside(repo_root, Path::new(cfg)) {
        return format!("CLAUDE_CONFIG_DIR ({cfg}) lies inside the repository under review: the engine's transcripts would change the tree; point it elsewhere");
    }
    let pd = projects_directory(config_dir, home, reported);
    if !pd.is_empty() && path_inside(repo_root, Path::new(&pd)) {
        return format!("the claude projectsDirectory ({pd}) lies inside the repository under review: the engine's transcripts would change the tree");
    }
    String::new()
}

/// (wave 4f, F25-2) The directory Claude Code writes its transcripts under: the projectsDirectory
/// `claude auth status` reported when it ran (`reported`, trimmed), else the one Claude Code
/// derives - `<config home>/projects`, the config home `CLAUDE_CONFIG_DIR` when set, else
/// `<home>/.claude`; `""` when neither is known.
pub fn projects_directory(config_dir: &str, home: &str, reported: &str) -> String {
    let r = reported.trim();
    if !r.is_empty() {
        return r.to_string();
    }
    let cfg = config_dir.trim();
    let base = if !cfg.is_empty() {
        PathBuf::from(cfg)
    } else if !home.trim().is_empty() {
        Path::new(home.trim()).join(".claude")
    } else {
        return String::new();
    };
    base.join("projects").to_string_lossy().to_string()
}

/// The home directory Claude Code (node's `os.homedir()`) resolves: `USERPROFILE` on Windows,
/// `HOME` elsewhere, each falling back to the other; `""` when neither is set.
fn home_dir() -> String {
    let (first, second) = if cfg!(windows) {
        ("USERPROFILE", "HOME")
    } else {
        ("HOME", "USERPROFILE")
    };
    [first, second]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.trim().is_empty())
        .unwrap_or_default()
}

/// Whether `p` lies inside `root` (or is it): lexically, as `Get-RepoRelativePath` decides (a
/// relative `p` against the current directory), or (wave 4f, F25-2) once the links on the way are
/// resolved - the deepest existing ancestor of `p` and the root canonicalised - so a junction or a
/// symbolic link into the repository is caught too.
pub fn path_inside(root: &Path, p: &Path) -> bool {
    let full = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    };
    if c3_core::paths::repo_relative(root, &full).is_some() {
        return true;
    }
    match (resolve_existing(root), resolve_existing(&full)) {
        (Some(r), Some(f)) => c3_core::paths::repo_relative(&r, &f).is_some(),
        _ => false,
    }
}

/// `p` with its deepest existing ancestor canonicalised (links resolved, the verbatim prefix
/// dropped) and the rest appended; `None` when no ancestor exists.
fn resolve_existing(p: &Path) -> Option<PathBuf> {
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = p.to_path_buf();
    loop {
        if let Ok(c) = std::fs::canonicalize(&cur) {
            let mut out = crate::providers::dunce_simplify(c);
            for part in rest.iter().rev() {
                out.push(part);
            }
            return Some(out);
        }
        rest.push(cur.file_name()?.to_os_string());
        cur = cur.parent()?.to_path_buf();
    }
}

/// The launcher's `--version` probe (`Get-ClaudeHarness`'s; wave 4f, F25-1: also the endpoint
/// preflight's proof that the launcher runs), run once per launcher per process in the child
/// environment of auth subscription, as the plugin's harness probe is (no endpoint token reaches a
/// launcher not yet proven).
#[derive(Debug, Clone, Default)]
pub struct VersionProbe {
    pub started: bool,
    /// Why it did not start (`""` when it did).
    pub why: String,
    pub timed_out: bool,
    /// The exit code (`-1` when it did not exit).
    pub exit: i32,
    /// The version token it printed (`""` = none).
    pub version: String,
}

/// The time the `--version` probe gets (`Get-ClaudeHarness`: 15 s).
pub const VERSION_PROBE_SEC: u64 = 15;

/// [`VersionProbe`] of `launcher`, cached per launcher; the cache stays locked while a probe runs,
/// so concurrent callers (a panel's members) start one probe per launcher.
pub fn version_probe(launcher: &str) -> VersionProbe {
    static C: OnceLock<Mutex<HashMap<String, VersionProbe>>> = OnceLock::new();
    let cache = C.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().ok();
    if let Some(v) = guard.as_ref().and_then(|c| c.get(launcher).cloned()) {
        return v;
    }
    let ver_re = re(r"^v?[0-9]+\.[0-9]+[0-9A-Za-z.+_-]{0,48}$");
    let cap = probe(
        launcher,
        &["--version"],
        VERSION_PROBE_SEC,
        &child_env("subscription", None),
    );
    let mut v = VersionProbe {
        started: cap.started,
        why: cap.why.clone(),
        timed_out: cap.timed_out,
        exit: cap.exit,
        version: String::new(),
    };
    if cap.started && !cap.timed_out && cap.exit == 0 {
        for tok in format!("{}\n{}", cap.out, cap.err).split_whitespace() {
            if ver_re.is_match(tok) {
                v.version = tok.to_string();
                break;
            }
        }
    }
    if let Some(c) = guard.as_mut() {
        c.insert(launcher.to_string(), v.clone());
    }
    v
}

/// (wave 4f, F25-1) Whether the launcher runs, by its `--version` probe: `None` when it started and
/// exited 0; else the credential result - not started or a non-zero exit: missing (the launcher
/// does not run: `--engine-exe` or the configured launcher names a wrong file); no exit within the
/// probe's time: unknown (not checked), as a hung `claude auth status` is.
pub fn launcher_verdict(v: &VersionProbe) -> Option<CredentialResult> {
    if !v.started {
        return Some(CredentialResult::missing(format!(
            "the claude launcher does not run - `claude --version` could not be started{}",
            if v.why.is_empty() {
                String::new()
            } else {
                format!(" ({})", v.why)
            }
        )));
    }
    if v.timed_out {
        return Some(CredentialResult::unknown(format!(
            "not checked - `claude --version` did not finish within {VERSION_PROBE_SEC} s"
        )));
    }
    if v.exit != 0 {
        return Some(CredentialResult::missing(format!(
            "the claude launcher does not run - `claude --version` exited {}",
            v.exit
        )));
    }
    None
}

/// [`launcher_verdict`] of `launcher`'s (cached) `--version` probe.
pub fn launcher_problem(launcher: &str) -> Option<CredentialResult> {
    launcher_verdict(&version_probe(launcher))
}

/// `Get-ClaudeHarness`: `claude-cli <version>` from `<launcher> --version` in the child
/// environment (15 s; the launcher's file metadata is not read by c3), else
/// `claude-cli (version unknown)`. Cached per launcher (the probe is [`version_probe`]).
pub fn harness(launcher: &str) -> String {
    if launcher.is_empty() {
        return "claude-cli (version unknown)".to_string();
    }
    let v = version_probe(launcher);
    if v.version.is_empty() {
        "claude-cli (version unknown)".to_string()
    } else {
        format!("claude-cli {}", v.version.trim_start_matches('v'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_core::credential::State;

    fn scratch(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let d = std::env::temp_dir().join(format!("c3-w4f-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A launcher that answers `--version` with `exit` (a batch file on Windows, a shell script
    /// elsewhere).
    fn script_launcher(dir: &Path, exit: i32) -> PathBuf {
        if cfg!(windows) {
            let p = dir.join("claude.cmd");
            std::fs::write(
                &p,
                format!("@echo 2.1.0-w4f (Claude Code)\r\n@exit /b {exit}\r\n"),
            )
            .unwrap();
            p
        } else {
            let p = dir.join("claude");
            std::fs::write(
                &p,
                format!("#!/bin/sh\necho '2.1.0-w4f (Claude Code)'\nexit {exit}\n"),
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            p
        }
    }

    /// A directory link `link` -> `target` (a junction on Windows, a symbolic link elsewhere).
    fn dir_link(link: &Path, target: &Path) {
        if cfg!(windows) {
            let st = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap();
            assert!(st.success(), "mklink /J");
        } else {
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, link).unwrap();
        }
    }

    fn endpoint(env_key: &str) -> ClaudeEndpoint {
        c3_core::claude::endpoint_from_value(
            &serde_json::json!({"base_url": "https://api.z.ai/api/anthropic", "env_key": env_key}),
            "",
        )
        .unwrap()
    }

    /// (F25-1) The launcher verdict of the `--version` probe: not started or a non-zero exit is
    /// missing, a hang is not checked, a clean exit passes.
    #[test]
    fn the_launcher_verdict_of_the_version_probe() {
        let not_started = VersionProbe {
            why: "%1 is not a valid Win32 application. (os error 193)".into(),
            exit: -1,
            ..Default::default()
        };
        let r = launcher_verdict(&not_started).unwrap();
        assert_eq!(r.state, State::Missing);
        assert_eq!(r.reason, "the claude launcher does not run - `claude --version` could not be started (%1 is not a valid Win32 application. (os error 193))");
        let failed = VersionProbe {
            started: true,
            exit: 3,
            ..Default::default()
        };
        assert_eq!(
            launcher_verdict(&failed).unwrap().reason,
            "the claude launcher does not run - `claude --version` exited 3"
        );
        let hung = VersionProbe {
            started: true,
            timed_out: true,
            exit: -1,
            ..Default::default()
        };
        let h = launcher_verdict(&hung).unwrap();
        assert_eq!(h.state, State::Unknown);
        assert_eq!(
            h.reason,
            "not checked - `claude --version` did not finish within 15 s"
        );
        let ok = VersionProbe {
            started: true,
            exit: 0,
            version: "2.1.0".into(),
            ..Default::default()
        };
        assert!(launcher_verdict(&ok).is_none());
    }

    /// (F25-1, RC1) A valid auth endpoint with its token variable set: a launcher that does not
    /// run (a file that is no program; one whose `--version` fails) is unavailable before any
    /// turn; a runnable one is available with the plugin's `env <NAME> set`.
    #[test]
    fn an_endpoint_launcher_that_does_not_run_is_unavailable() {
        let d = scratch("launcher");
        let var = format!("C3_W4F_TOKEN_{}", std::process::id());
        std::env::set_var(&var, "fake-token-w4f-unit");
        let ep = endpoint(&var);
        // not a program at all
        let junk = d.join("not-claude.exe");
        std::fs::write(&junk, "this is not a program\n").unwrap();
        let junk_s = junk.to_string_lossy().to_string();
        let r = sign_in(&junk_s, 0, "endpoint", Some(&ep));
        assert_eq!(r.state, State::Missing, "{}", r.reason);
        assert!(
            r.reason.starts_with(
                "the claude launcher does not run - `claude --version` could not be started"
            ),
            "{}",
            r.reason
        );
        // a program whose `--version` fails
        let bad_dir = d.join("bad");
        std::fs::create_dir_all(&bad_dir).unwrap();
        let bad = script_launcher(&bad_dir, 3);
        let b = sign_in(&bad.to_string_lossy(), 0, "endpoint", Some(&ep));
        assert_eq!(b.state, State::Missing, "{}", b.reason);
        assert_eq!(
            b.reason,
            "the claude launcher does not run - `claude --version` exited 3"
        );
        // a runnable launcher: available, and its version is the harness string (one probe)
        let good_dir = d.join("good");
        std::fs::create_dir_all(&good_dir).unwrap();
        let good = script_launcher(&good_dir, 0);
        let good_s = good.to_string_lossy().to_string();
        let g = sign_in(&good_s, 0, "endpoint", Some(&ep));
        assert_eq!(g.state, State::Ok, "{}", g.reason);
        assert_eq!(g.reason, format!("env {var} set"));
        assert_eq!(harness(&good_s), "claude-cli 2.1.0-w4f");
        // -NoNetwork starts nothing: the local check answers, even for the junk launcher
        let n = engine_credential(&junk_s, "endpoint", Some(&ep), None, true);
        assert_eq!(n.state, State::Ok);
        assert_eq!(n.reason, format!("env {var} set"));
        std::env::remove_var(&var);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// (F25-2, RC2) The transcript guard in every auth mode: the projectsDirectory `claude auth
    /// status` reported, else the one Claude Code derives (`<CLAUDE_CONFIG_DIR or ~/.claude>/
    /// projects`), links resolved - refused inside the repository with the plugin's text, allowed
    /// outside.
    #[test]
    fn the_projects_directory_inside_the_repository_is_refused_in_every_auth_mode() {
        let d = scratch("projdir");
        let repo = d.join("repo");
        std::fs::create_dir_all(repo.join("transcripts")).unwrap();
        let cfg_out = d.join("claude-config");
        std::fs::create_dir_all(&cfg_out).unwrap();
        let home_out = d.join("home");
        std::fs::create_dir_all(&home_out).unwrap();
        let cfg_s = cfg_out.to_string_lossy().to_string();
        let home_s = home_out.to_string_lossy().to_string();
        // outside: allowed (the derived <cfg>/projects; the derived ~/.claude/projects)
        assert_eq!(launch_problem_of(&repo, &cfg_s, &home_s, ""), "");
        assert_eq!(launch_problem_of(&repo, "", &home_s, ""), "");
        assert_eq!(
            projects_directory(&cfg_s, &home_s, ""),
            cfg_out.join("projects").to_string_lossy()
        );
        assert_eq!(
            projects_directory("", &home_s, ""),
            home_out.join(".claude").join("projects").to_string_lossy()
        );
        assert_eq!(projects_directory("", "", ""), "");
        // reported by `claude auth status` (subscription, api-key) inside: refused
        let inside = repo.join("projects").to_string_lossy().to_string();
        assert_eq!(
            launch_problem_of(&repo, &cfg_s, &home_s, &inside),
            format!("the claude projectsDirectory ({inside}) lies inside the repository under review: the engine's transcripts would change the tree")
        );
        // CLAUDE_CONFIG_DIR inside: the plugin's first refusal
        let cfg_in = repo.join("cfg").to_string_lossy().to_string();
        assert_eq!(
            launch_problem_of(&repo, &cfg_in, &home_s, ""),
            format!("CLAUDE_CONFIG_DIR ({cfg_in}) lies inside the repository under review: the engine's transcripts would change the tree; point it elsewhere")
        );
        // no CLAUDE_CONFIG_DIR and the home inside the repository: the derived directory is refused
        let home_in = repo.join("home");
        let derived_in = home_in
            .join(".claude")
            .join("projects")
            .to_string_lossy()
            .to_string();
        assert_eq!(
            launch_problem_of(&repo, "", &home_in.to_string_lossy(), ""),
            format!("the claude projectsDirectory ({derived_in}) lies inside the repository under review: the engine's transcripts would change the tree")
        );
        // CLAUDE_CONFIG_DIR outside, its projects directory a link into the repository: refused
        let cfg_link = d.join("claude-config-linked");
        std::fs::create_dir_all(&cfg_link).unwrap();
        dir_link(&cfg_link.join("projects"), &repo.join("transcripts"));
        let link_s = cfg_link.to_string_lossy().to_string();
        let pd = cfg_link.join("projects").to_string_lossy().to_string();
        assert_eq!(
            launch_problem_of(&repo, &link_s, &home_s, ""),
            format!("the claude projectsDirectory ({pd}) lies inside the repository under review: the engine's transcripts would change the tree")
        );
        assert!(path_inside(
            &repo,
            &cfg_link.join("projects").join("a-session")
        ));
        assert!(!path_inside(&repo, &cfg_out.join("projects")));
        let _ = std::fs::remove_dir(cfg_link.join("projects"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
