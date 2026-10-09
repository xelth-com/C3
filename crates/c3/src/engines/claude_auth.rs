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
/// its JSON read BEFORE its exit code; auth endpoint: no probe - the endpoint and its token
/// variable (`ok: env <NAME> set`). One probe per `launcher|auth` per process.
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
/// set now; auth endpoint: the endpoint and its token variable); auth endpoint is checked locally
/// every time (`ok: env <NAME> set` - no `claude auth status`, under `-NoNetwork` too); then a
/// usable reply of this endpoint within 60 minutes evidences the sign-in; `-NoNetwork` (the
/// SessionStart hook) starts nothing; else `claude auth status` (`CODEX_CONSULT_TEST_LOGIN_TIMEOUT`
/// shortens the 15 s in test mode).
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
/// repository under review - `CLAUDE_CONFIG_DIR`, and the projectsDirectory the sign-in reported.
pub fn launch_problem(repo_root: &Path, launcher: &str, auth: &str) -> String {
    let inside = |p: &str| -> bool {
        let pb = PathBuf::from(p.trim());
        let full = if pb.is_absolute() {
            pb
        } else {
            std::env::current_dir().unwrap_or_default().join(pb)
        };
        c3_core::paths::repo_relative(repo_root, &full).is_some()
    };
    if let Ok(cfg) = std::env::var("CLAUDE_CONFIG_DIR") {
        if !cfg.trim().is_empty() && inside(&cfg) {
            return format!("CLAUDE_CONFIG_DIR ({}) lies inside the repository under review: the engine's transcripts would change the tree; point it elsewhere", cfg.trim());
        }
    }
    if let Some(info) = sign_in_info(launcher, auth) {
        let pd = info.projects_directory.trim().to_string();
        if !pd.is_empty() && inside(&pd) {
            return format!("the claude projectsDirectory ({pd}) lies inside the repository under review: the engine's transcripts would change the tree");
        }
    }
    String::new()
}

/// `Get-ClaudeHarness`: `claude-cli <version>` from `<launcher> --version` in the child
/// environment (15 s; the launcher's file metadata is not read by c3), else
/// `claude-cli (version unknown)`. Cached per launcher.
pub fn harness(launcher: &str) -> String {
    static C: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let cache = C.get_or_init(|| Mutex::new(HashMap::new()));
    if launcher.is_empty() {
        return "claude-cli (version unknown)".to_string();
    }
    if let Some(h) = cache.lock().ok().and_then(|c| c.get(launcher).cloned()) {
        return h;
    }
    let ver_re = re(r"^v?[0-9]+\.[0-9]+[0-9A-Za-z.+_-]{0,48}$");
    let mut ver = String::new();
    let cap = probe(
        launcher,
        &["--version"],
        15,
        &child_env("subscription", None),
    );
    if cap.started && !cap.timed_out && cap.exit == 0 {
        for tok in format!("{}\n{}", cap.out, cap.err).split_whitespace() {
            if ver_re.is_match(tok) {
                ver = tok.to_string();
                break;
            }
        }
    }
    let h = if ver.is_empty() {
        "claude-cli (version unknown)".to_string()
    } else {
        format!("claude-cli {}", ver.trim_start_matches('v'))
    };
    if let Ok(mut c) = cache.lock() {
        c.insert(launcher.to_string(), h.clone());
    }
    h
}
