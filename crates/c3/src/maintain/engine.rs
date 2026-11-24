//! Per-engine maintenance adapters: how each CLI reports its installed version, how its latest
//! version is learned, and how it is updated and rolled back. The real command shapes were
//! confirmed against the installed CLIs (2026-11):
//!
//! * **codex** — npm global `@openai/codex`. Latest: `npm view @openai/codex version` (release date
//!   from `npm view @openai/codex time --json`); update `npm install -g @openai/codex@<v>`; rollback
//!   the same to the previous version. Only when codex is an npm global install; otherwise
//!   report-only. (The CLI also has a `codex update`, but the design pins the npm path so the
//!   rollback is deterministic.)
//! * **grok** — `grok update --check --json` → `{currentVersion,latestVersion,updateAvailable,…}`;
//!   update `grok update`; rollback `grok update --version <prev>` (the `--version` pin exists, so
//!   grok has a rollback path).
//! * **kimi** — no self-update subcommand; the vendor installer. Latest: GET
//!   `https://code.kimi.ai/kimi-code/latest`. Update: download `install.ps1` (Windows) / `install.sh`
//!   (POSIX) over https to the state dir and run it; `KIMI_VERSION=<v>` pins, which is the rollback.
//! * **agy** — `agy update` self-updates, but `agy update --help` exposes no check and no version
//!   pin, so there is no way to learn the latest beforehand and no rollback: report-only.
//! * **muse / claude** — both update themselves (claude is also the coordinator host): the version is
//!   recorded, never updated.

use std::path::Path;

use chrono::{DateTime, Utc};

use super::env::{Env, GetPolicy, Redirects, RunResult, HTTP_BODY_CAP};
use super::version::{parse_cli_version, CliVersion};

/// What a maintenance pass may do with an engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    /// The engine can be updated at all (false for muse/claude: record-only).
    pub updatable: bool,
    /// The latest version can be learned before installing (false for agy: no check command).
    pub has_check: bool,
    /// A specific previous version can be reinstalled (false for agy).
    pub has_rollback: bool,
}

/// The engines a pass walks, in order.
pub const ENGINES: &[&str] = &["codex", "grok", "kimi", "agy", "muse", "claude"];

/// Resolve `npm` to a runnable launcher. On Windows the node install ships `npm.ps1`, `npm.cmd`
/// and an extensionless `npm` (a bash script) in the same directory; `CreateProcess`/`Command`
/// cannot run the `.ps1` or the extensionless shell script, so we must pick `npm.cmd` (or `npm.exe`)
/// explicitly - the bare name would resolve to the first file, the unrunnable `npm`. Falls back to
/// the bare name elsewhere.
fn npm() -> String {
    if cfg!(windows) {
        for n in ["npm.cmd", "npm.exe", "npm.bat"] {
            if let Some(p) = crate::providers::find_on_path(n) {
                return p;
            }
        }
    }
    crate::providers::find_on_path("npm").unwrap_or_else(|| "npm".to_string())
}

/// The maintenance capabilities of an engine (an unknown name is record-only).
pub fn caps(name: &str) -> Caps {
    match name {
        "codex" | "grok" | "kimi" => Caps {
            updatable: true,
            has_check: true,
            has_rollback: true,
        },
        "agy" => Caps {
            updatable: true,
            has_check: false,
            has_rollback: false,
        },
        // muse and claude update themselves; C3 records the version and never updates.
        _ => Caps {
            updatable: false,
            has_check: false,
            has_rollback: false,
        },
    }
}

/// Resolve the engine's launcher exactly as the engine does (`None`/`""` → not installed).
pub fn resolve_launcher(name: &str) -> Option<String> {
    match crate::providers::resolve_engine_launcher(name, "") {
        Ok(Some(l)) if !l.is_empty() => Some(l),
        _ => None,
    }
}

/// The installed release from `<launcher> --version` (the first semver-like token, read as an exact
/// release identity - (F62-1) `grok 1.0.80-rc.1` is `1.0.80-rc.1`, never `1.0.80`), or `None`.
pub fn installed_version(env: &dyn Env, launcher: &str) -> Option<CliVersion> {
    // A version probe runs with the operator's provider secrets stripped (run_clean).
    let r = env.run_clean(launcher, &["--version"], &[], None);
    parse_cli_version(&r.combined())
}

/// What a check learned about the latest available version.
#[derive(Debug, Clone, Default)]
pub struct Latest {
    /// The latest release (e.g. `0.170.0`, or exactly `0.170.0-alpha.1` - the version an update
    /// pins), or `None` when it could not be learned.
    pub version: Option<CliVersion>,
    /// The release date of `version` when it can be known (npm `time`); else `None`.
    pub released: Option<DateTime<Utc>>,
    /// A one-line note when no check is possible (report-only engines); else `""`.
    pub note: String,
}

/// Learn the latest available version for `name`.
pub fn check_latest(env: &dyn Env, name: &str, launcher: &str) -> Latest {
    match name {
        "codex" => check_codex(env, launcher),
        "grok" => check_grok(env, launcher),
        "kimi" => check_kimi(env),
        _ => Latest {
            note: "no check command".to_string(),
            ..Default::default()
        },
    }
}

fn check_codex(env: &dyn Env, launcher: &str) -> Latest {
    if !codex_is_npm_global(env, launcher) {
        return Latest {
            note: "not an npm global install - report only".to_string(),
            ..Default::default()
        };
    }
    let ver = env.run_clean(&npm(), &["view", "@openai/codex", "version"], &[], None);
    if !ver.ok() {
        return Latest {
            note: format!("npm view failed ({})", ver.fail_reason()),
            ..Default::default()
        };
    }
    let version = parse_cli_version(ver.stdout.trim());
    if version.is_none() {
        return Latest {
            note: "npm view returned no version".to_string(),
            ..Default::default()
        };
    }
    let released = version.as_ref().and_then(|v| {
        let out = env.run_clean(
            &npm(),
            &["view", "@openai/codex", "time", "--json"],
            &[],
            None,
        );
        if !out.ok() {
            return None;
        }
        let data: serde_json::Value = serde_json::from_str(out.stdout.trim()).ok()?;
        // (F62-1) keyed by the exact release: a pre-release's own date, never its final release's
        let stamp = data.get(v.text()).and_then(|x| x.as_str())?;
        DateTime::parse_from_rfc3339(stamp)
            .ok()
            .map(|d| d.with_timezone(&Utc))
    });
    Latest {
        version,
        released,
        note: String::new(),
    }
}

/// Whether the installed codex is an npm global package (so the npm update/rollback path applies).
fn codex_is_npm_global(env: &dyn Env, launcher: &str) -> bool {
    let lower = launcher.to_ascii_lowercase();
    if lower.contains("node_modules") || lower.replace('\\', "/").contains("/npm/") {
        return true;
    }
    let r = env.run_clean(&npm(), &["ls", "-g", "@openai/codex"], &[], None);
    r.ok() && r.stdout.contains("@openai/codex@")
}

fn check_grok(env: &dyn Env, launcher: &str) -> Latest {
    let r = env.run_clean(launcher, &["update", "--check", "--json"], &[], None);
    if !r.ok() {
        return Latest {
            note: "grok update --check failed".to_string(),
            ..Default::default()
        };
    }
    let data: serde_json::Value = match serde_json::from_str(r.stdout.trim()) {
        Ok(v) => v,
        Err(_) => {
            return Latest {
                note: "grok update --check gave no JSON".to_string(),
                ..Default::default()
            }
        }
    };
    let version = data
        .get("latestVersion")
        .and_then(|v| v.as_str())
        .and_then(parse_cli_version);
    Latest {
        version,
        released: None,
        note: String::new(),
    }
}

/// The kimi latest-version marker URL (the vendor installer's `latest` sibling).
pub const KIMI_LATEST_URL: &str = "https://code.kimi.ai/kimi-code/latest";
/// The kimi vendor installer URL (Windows).
pub const KIMI_INSTALL_PS1: &str = "https://code.kimi.ai/kimi-code/install.ps1";
/// The kimi vendor installer URL (POSIX).
pub const KIMI_INSTALL_SH: &str = "https://code.kimi.ai/kimi-code/install.sh";

fn check_kimi(env: &dyn Env) -> Latest {
    match env.http_get(
        KIMI_LATEST_URL,
        &[],
        GetPolicy {
            timeout: std::time::Duration::from_secs(5),
            redirects: Redirects::Installer,
            cap: HTTP_BODY_CAP,
        },
    ) {
        // The marker answers 200 with the bare version and no text content-type, so the body is
        // read as bytes (via http_get) and trimmed before validating it is a version.
        Ok((status, body)) if (200..300).contains(&status) => {
            match parse_cli_version(body.trim()) {
                Some(v) => Latest {
                    version: Some(v),
                    released: None,
                    note: String::new(),
                },
                None => Latest {
                    note: "the kimi latest marker was not a version".to_string(),
                    ..Default::default()
                },
            }
        }
        Ok((status, _)) => Latest {
            note: format!("the kimi latest marker returned {status}"),
            ..Default::default()
        },
        Err(e) => Latest {
            note: format!("the kimi latest marker could not be read ({e})"),
            ..Default::default()
        },
    }
}

/// Run the update of `name` to `to` (a version string). Returns the command's result, or `Err`
/// with a one-line reason when the update could not even be launched.
pub fn do_update(
    env: &dyn Env,
    name: &str,
    launcher: &str,
    to: &str,
    state_dir: &Path,
) -> Result<RunResult, String> {
    match name {
        "codex" => Ok(env.run_clean(
            &npm(),
            &["install", "-g", &format!("@openai/codex@{to}")],
            &[],
            None,
        )),
        // Pin grok to the CHECKED version (confirmed flag: `grok update --help`), so a release that
        // appeared between the check and the update cannot be installed behind the version the pass
        // reasoned about; the caller re-reads `--version` and requires this target.
        "grok" => Ok(env.run_clean(launcher, &["update", "--version", to], &[], None)),
        "kimi" => run_kimi_installer(env, state_dir, Some(to)),
        other => Err(format!("{other} has no update path")),
    }
}

/// Roll `name` back to the previous version `to`. `None` when the engine has no rollback path.
pub fn do_rollback(
    env: &dyn Env,
    name: &str,
    launcher: &str,
    to: &str,
    state_dir: &Path,
) -> Option<Result<RunResult, String>> {
    match name {
        "codex" => Some(Ok(env.run_clean(
            &npm(),
            &["install", "-g", &format!("@openai/codex@{to}")],
            &[],
            None,
        ))),
        "grok" => Some(Ok(env.run_clean(
            launcher,
            &["update", "--version", to],
            &[],
            None,
        ))),
        "kimi" => Some(run_kimi_installer(env, state_dir, Some(to))),
        _ => None,
    }
}

/// Download the kimi vendor installer over https to the state dir and run it, pinning `version`
/// through `KIMI_VERSION` when given. Windows uses `install.ps1` via powershell; POSIX uses
/// `install.sh` via sh.
fn run_kimi_installer(
    env: &dyn Env,
    state_dir: &Path,
    version: Option<&str>,
) -> Result<RunResult, String> {
    let (url, file, program): (&str, &str, &str) = if cfg!(windows) {
        (KIMI_INSTALL_PS1, "kimi-install.ps1", "powershell")
    } else {
        (KIMI_INSTALL_SH, "kimi-install.sh", "sh")
    };
    let (status, body) = env
        .http_get(
            url,
            &[],
            GetPolicy {
                timeout: std::time::Duration::from_secs(30),
                redirects: Redirects::Installer,
                cap: HTTP_BODY_CAP,
            },
        )
        .map_err(|e| format!("could not download the kimi installer ({e})"))?;
    if !(200..300).contains(&status) {
        return Err(format!("the kimi installer download returned {status}"));
    }
    let script = state_dir.join(file);
    // The vendor script is UTF-8 without a byte-order mark and carries a few non-ASCII characters.
    // Windows PowerShell 5.1 reads such a `-File` in the ANSI code page, where the bytes of a dash
    // turn into a quote character and can break the parse - so the mark is added on Windows.
    let mut bytes: Vec<u8> = Vec::with_capacity(body.len() + 3);
    if cfg!(windows) && !body.starts_with('\u{feff}') {
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    bytes.extend_from_slice(body.as_bytes());
    std::fs::write(&script, &bytes)
        .map_err(|e| format!("could not write the kimi installer ({e})"))?;
    let script_s = script.to_string_lossy().to_string();
    let pin: Vec<(&str, &str)> = version
        .map(|v| vec![("KIMI_VERSION", v)])
        .unwrap_or_default();
    let args: Vec<&str> = if cfg!(windows) {
        vec![
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script_s,
        ]
    } else {
        vec![script_s.as_str()]
    };
    Ok(env.run_clean(program, &args, &pin, Some(state_dir)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake Env whose HTTP GET fails (as a body over the cap, or any download error, makes it) and
    /// which records every command it was asked to run.
    struct FailingHttpEnv {
        ran: std::cell::RefCell<Vec<String>>,
    }
    impl Env for FailingHttpEnv {
        fn run(
            &self,
            program: &str,
            args: &[&str],
            _env: &[(&str, &str)],
            _cwd: Option<&Path>,
        ) -> RunResult {
            self.ran
                .borrow_mut()
                .push(format!("{program} {}", args.join(" ")));
            RunResult {
                code: Some(0),
                ..Default::default()
            }
        }
        fn http_get(
            &self,
            _url: &str,
            _h: &[(&str, &str)],
            _p: GetPolicy,
        ) -> Result<(u16, String), String> {
            Err(
                "the response body exceeded the cap (a truncated download is never used)"
                    .to_string(),
            )
        }
    }

    #[test]
    fn a_truncated_kimi_download_is_never_written_or_run() {
        let dir = std::env::temp_dir().join(format!(
            "c3-mnt-kimi-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let env = FailingHttpEnv {
            ran: std::cell::RefCell::new(Vec::new()),
        };
        let r = do_update(&env, "kimi", "kimi", "1.2.3", &dir);
        assert!(
            r.is_err(),
            "a failed/oversized download is an error, not a partial install"
        );
        // nothing was written to disk and no installer was executed
        assert!(!dir.join("kimi-install.ps1").exists());
        assert!(!dir.join("kimi-install.sh").exists());
        assert!(
            env.ran.borrow().is_empty(),
            "the installer was never run: {:?}",
            env.ran.borrow()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn caps_per_engine() {
        assert!(caps("codex").has_rollback);
        assert!(caps("grok").has_rollback);
        assert!(caps("kimi").has_rollback);
        assert!(caps("agy").updatable && !caps("agy").has_check && !caps("agy").has_rollback);
        assert!(!caps("muse").updatable);
        assert!(!caps("claude").updatable);
    }
}
