//! Argument validation and the self-contained resolutions (`codex-consult.ps1:838-1105`).
//!
//! These are the checks and defaults that do not need the resolved identity, the config or
//! the roster: the purpose/effort/sandbox/mode value sets, the `-Range` purpose gate and
//! single-revision refusal, `-NativeEffort` excluding `-Effort`, `-Thread` needing a mode,
//! `-FormatRetry`/`-DenialRetry` bounds, `-SchemaTransport` value + `-Raw` exclusion, the
//! `-CodexConfig` identity-key refusals (via [`c3_core::roster::convert_from_codex_config_items`]),
//! the per-purpose word/timeout defaults and the continuation budget. Refusals carry the
//! plugin's exact wording; each is a usage error (exit 1).
//!
//! The engine-specific refusals for `--engine agy|muse`, `--panel*`, `--denial-retry`,
//! `--max-model-steps`, `--engine-exe` and the reserved detach flags are handled here too,
//! refused with a clear milestone note (agy/muse land in M2d, panel in M4, detach in R12).

use super::prompt::presets;

/// The plain options a `c3 consult` invocation carries (filled from clap in `cli::consult`).
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub task: String,
    pub collab_dir: String,
    pub mode: String,
    pub thread: String,
    pub brief: String,
    pub prompt: String,
    pub model: String,
    pub purpose: String,
    pub effort: String,
    pub sandbox: String,
    pub max_words: i64,
    pub timeout_sec: i64,
    /// The raw `--continue-sec`; the default sentinel is `-1`.
    pub continue_sec: i64,
    /// Whether `--continue-sec` was given explicitly (a negative value is then a refusal).
    pub continue_sec_given: bool,
    pub range: String,
    pub reply_name: String,
    pub artifacts: Vec<String>,
    pub raw: bool,
    pub codex_exe: String,
    pub provider: String,
    pub native_effort: String,
    pub off_peak_only: bool,
    pub skip_preflight: bool,
    pub codex_config: Vec<String>,
    pub schema_transport: String,
    /// Telemetry `--telemetry on|off`: `Some(false)` for off, `Some(true)` for on, `None`
    /// when the flag was absent (env `CODEX_CONSULT_TELEMETRY=off` disables regardless).
    pub telemetry: Option<bool>,
    /// `--format-retry` as an integer 0|1 (default 1).
    pub format_retry: i64,
    pub dry_run: bool,

    // Parsed-but-refused surface (see module docs).
    pub engine: String,
    pub engine_exe: String,
    /// `--denial-retry` value (0|1; default 1). Meaningful for the agy engine; the dry-run
    /// block prints its state. Codex/muse ignore it.
    pub denial_retry: i64,
    pub max_model_steps: i64,
    pub panel: bool,
    pub panel_all: bool,
    pub panel_concurrency_given: bool,
    pub detach: bool,
    pub status: bool,
    pub list: bool,
    pub wait: bool,
    pub prune: bool,
}

/// The self-contained resolutions after validation succeeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// `-Raw` after the chore fold (`-Purpose chore` forces raw).
    pub raw: bool,
    pub max_words: u32,
    pub timeout_sec: i64,
    /// `purpose` | `explicit`.
    pub timeout_source: String,
    pub continue_sec: i64,
    /// Whether a format-repair turn is enabled for this run.
    pub repair_enabled: bool,
    /// The `output-schema`|`prompt-only` override (empty = caps-v1 default).
    pub transport_override: String,
    /// The expanded `-c` items (identity/effort keys already refused).
    pub extra_config: Vec<String>,
    /// The verdict rule phrase for the resolved purpose.
    pub verdict_rule: String,
    pub purpose_label: String,
}

const VALID_EFFORTS: [&str; 4] = ["low", "medium", "high", "xhigh"];
const VALID_SANDBOXES: [&str; 2] = ["read-only", "workspace-write"];
const RANGE_PURPOSES: [&str; 2] = ["diff-review", "acceptance"];

fn is_slug(s: &str) -> bool {
    c3_core::task_slug::is_slug(s)
}

/// Validate the options and compute the self-contained resolutions. `Err` is a refusal
/// message to print (exit 1); the message text matches the plugin's `Stop-WithError`.
pub fn validate(o: &Options, home_dir: Option<&str>) -> Result<Resolved, String> {
    // Refused surface first (milestone / engine scope).
    if o.panel || o.panel_all {
        return Err(
            "-Panel is a milestone 4 feature; c3 consult runs one reviewer (drop -Panel).".into(),
        );
    }
    if o.panel_concurrency_given {
        return Err(
            "-PanelConcurrency is a milestone 4 (panel) feature; c3 consult runs one reviewer (drop -PanelConcurrency)."
                .into(),
        );
    }
    // `-Engine`, `-EngineExe`, `-DenialRetry` and `-MaxModelSteps` are engine-aware from M2d on
    // (engine selection, the capability refusals and the `-EngineExe` binding live in
    // `orchestrate::build_context`, after the roster fixes the engine). The only self-contained
    // check here is `-MaxModelSteps`'s sign (`codex-consult.ps1:1770`).
    if o.max_model_steps < 0 {
        return Err(format!(
            "-MaxModelSteps must be a positive integer (got {}); omit it for the muse CLI's own default.",
            o.max_model_steps
        ));
    }
    if o.detach || o.status || o.list || o.wait || o.prune {
        return Err("--detach/--status/--list/--wait/--prune are reserved (plugin R12) and not implemented in c3 yet.".into());
    }

    // Value sets and bounds (plugin order).
    if !o.effort.is_empty() && !VALID_EFFORTS.contains(&o.effort.as_str()) {
        return Err(format!(
            "-Effort must be one of: {} (got '{}').",
            VALID_EFFORTS.join(", "),
            o.effort
        ));
    }
    if !o.purpose.is_empty() && !presets::VALID.contains(&o.purpose.as_str()) {
        return Err(format!(
            "-Purpose must be one of: {} (got '{}').",
            presets::VALID.join(", "),
            o.purpose
        ));
    }
    if o.max_words < 0 {
        return Err(format!(
            "-MaxWords must be greater than 0 (got {}).",
            o.max_words
        ));
    }
    if o.timeout_sec < 0 {
        return Err(format!(
            "-TimeoutSec must be greater than 0 (got {}).",
            o.timeout_sec
        ));
    }
    if o.continue_sec_given && o.continue_sec < 0 {
        return Err(format!(
            "-ContinueSec must be 0 (no continuation after a timeout kill) or a number of seconds (got {}).",
            o.continue_sec
        ));
    }
    if !o.range.is_empty() && !RANGE_PURPOSES.contains(&o.purpose.as_str()) {
        return Err(format!(
            "-Range goes with -Purpose diff-review or acceptance (got {}): it sizes a review of that range.",
            if o.purpose.is_empty() {
                "no -Purpose".to_string()
            } else {
                format!("-Purpose {}", o.purpose)
            }
        ));
    }
    if !o.range.is_empty() && !is_range_pair(&o.range) {
        return Err(format!(
            "-Range '{}' must be a two-point range (base..head or base...head); a single revision would measure the working tree.",
            o.range
        ));
    }
    if o.sandbox == "danger-full-access" {
        return Err("-Sandbox danger-full-access is refused: consultations run without write access to your machine.".into());
    }
    if !o.sandbox.is_empty() && !VALID_SANDBOXES.contains(&o.sandbox.as_str()) {
        return Err(format!(
            "-Sandbox must be one of: {} (got '{}').",
            VALID_SANDBOXES.join(", "),
            o.sandbox
        ));
    }
    if !o.mode.is_empty() && !["new", "resume", "fork"].contains(&o.mode.as_str()) {
        return Err(format!(
            "-Mode must be one of: new, resume, fork (got '{}').",
            o.mode
        ));
    }
    if o.brief.is_empty() && o.prompt.is_empty() {
        return Err("Give at least one of -Brief <path> or -Prompt <text>.".into());
    }
    if !o.reply_name.is_empty() && !is_slug(&o.reply_name) {
        return Err("-ReplyName must be a slug (letters, digits, dot, dash, underscore).".into());
    }
    if o.task.is_empty() || !is_slug(&o.task) {
        return Err("-Task must be a slug (letters, digits, dot, dash, underscore).".into());
    }
    if !o.native_effort.is_empty() {
        if !o.effort.is_empty() {
            return Err("-Effort and -NativeEffort exclude each other: -Effort is mapped through the endpoint's effort vocabulary, -NativeEffort is sent verbatim.".into());
        }
        if !plain_token(&o.native_effort) {
            return Err(format!(
                "-NativeEffort must be a plain token (letters, digits, dot, dash, underscore; got '{}').",
                o.native_effort
            ));
        }
    }
    // Only an EXPLICIT `-Mode new` with `-Thread` is refused here (matching `Select-ParentThread`,
    // `if ($Mode -eq 'new')`); an empty/auto mode with `-Thread` falls through to the parent walk,
    // which defaults the mode to fork.
    if !o.thread.is_empty() && o.mode == "new" {
        return Err(
            "-Thread needs -Mode fork or resume (-Mode new always starts a fresh thread).".into(),
        );
    }

    // -CodexConfig expansion + identity/effort-key refusal.
    let (extra_config, cfg_err) =
        c3_core::roster::convert_from_codex_config_items(&o.codex_config, "-CodexConfig", home_dir);
    if !cfg_err.is_empty() {
        return Err(cfg_err);
    }

    // chore folds to raw.
    let raw = o.raw || o.purpose == "chore";

    // -SchemaTransport value + -Raw exclusion. `native` is a valid value here (the agy/muse
    // engines take it; the "for the agy and muse engines" refusal for codex, and the
    // "output-schema is refused" refusal for an engine, live in `orchestrate::build_context`,
    // which knows the selected engine). `codex-consult.ps1:1754`.
    let transport_override = o.schema_transport.trim().to_lowercase();
    if !transport_override.is_empty() {
        if !["output-schema", "prompt-only", "native"].contains(&transport_override.as_str()) {
            return Err(format!(
                "-SchemaTransport must be output-schema or prompt-only (got '{transport_override}'); omit it to use what caps-v1 declares for the endpoint."
            ));
        }
        if raw {
            let chore = if o.purpose == "chore" {
                " (-Purpose chore is a plain-text consultation)"
            } else {
                ""
            };
            return Err(format!(
                "-SchemaTransport does not apply to -Raw{chore} (a raw consultation sends no reply schema)."
            ));
        }
    }

    // -FormatRetry bounds (0|1); off for -Raw/chore.
    if o.format_retry != 0 && o.format_retry != 1 {
        return Err(format!(
            "-FormatRetry must be 0 or 1 (got {}): at most one format-repair turn per consultation.",
            o.format_retry
        ));
    }
    let repair_enabled = o.format_retry == 1 && !raw;

    // Timeout / continuation defaults.
    let (timeout_sec, timeout_source) = if o.timeout_sec == 0 {
        (presets::timeout(&o.purpose), "purpose".to_string())
    } else {
        (o.timeout_sec, "explicit".to_string())
    };
    let continue_sec = if o.continue_sec < 0 {
        timeout_sec.min(900)
    } else {
        o.continue_sec
    };

    let max_words = if o.max_words > 0 {
        o.max_words as u32
    } else {
        presets::words(&o.purpose)
    };

    Ok(Resolved {
        raw,
        max_words,
        timeout_sec,
        timeout_source,
        continue_sec,
        repair_enabled,
        transport_override,
        extra_config,
        verdict_rule: presets::verdict_rule(&o.purpose).to_string(),
        purpose_label: if o.purpose.is_empty() {
            "none".to_string()
        } else {
            o.purpose.clone()
        },
    })
}

/// A `-Range` is a two-point range (`a..b` / `a...b`) with both endpoints present.
fn is_range_pair(spec: &str) -> bool {
    // The plugin refuses a single revision (no `..`) and `HEAD` alone.
    let sep = if spec.contains("...") {
        "..."
    } else if spec.contains("..") {
        ".."
    } else {
        return false;
    };
    let mut parts = spec.splitn(2, sep);
    let a = parts.next().unwrap_or("").trim();
    let b = parts.next().unwrap_or("").trim();
    !a.is_empty() && !b.is_empty()
}

fn plain_token(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_opts() -> Options {
        Options {
            task: "t".into(),
            collab_dir: ".collab".into(),
            prompt: "x".into(),
            format_retry: 1,
            continue_sec: -1,
            ..Default::default()
        }
    }

    #[test]
    fn happy_defaults() {
        let r = validate(&ok_opts(), None).unwrap();
        assert!(!r.raw);
        assert_eq!(r.max_words, 700);
        assert_eq!(r.timeout_sec, 900); // no purpose → 900
        assert_eq!(r.timeout_source, "purpose");
        assert_eq!(r.continue_sec, 900);
        assert!(r.repair_enabled);
        assert_eq!(r.verdict_rule, "ADVISE");
    }

    #[test]
    fn purpose_defaults_flow() {
        let mut o = ok_opts();
        o.purpose = "acceptance".into();
        let r = validate(&o, None).unwrap();
        assert_eq!(r.timeout_sec, 3600);
        assert_eq!(r.max_words, 900);
        assert_eq!(r.verdict_rule, "ACCEPT, HOLD or REJECT");
    }

    #[test]
    fn chore_folds_to_raw_and_disables_repair() {
        let mut o = ok_opts();
        o.purpose = "chore".into();
        let r = validate(&o, None).unwrap();
        assert!(r.raw);
        assert!(!r.repair_enabled);
        assert_eq!(r.timeout_sec, 600);
    }

    #[test]
    fn explicit_timeout_and_continue() {
        let mut o = ok_opts();
        o.timeout_sec = 120;
        o.continue_sec = 0;
        o.continue_sec_given = true;
        let r = validate(&o, None).unwrap();
        assert_eq!(r.timeout_source, "explicit");
        assert_eq!(r.continue_sec, 0);
    }

    #[test]
    fn refusals() {
        let bad = |mut f: Box<dyn FnMut(&mut Options)>| {
            let mut o = ok_opts();
            f(&mut o);
            validate(&o, None).unwrap_err()
        };
        assert!(bad(Box::new(|o| o.purpose = "nope".into())).contains("-Purpose must be one of"));
        assert!(bad(Box::new(|o| o.sandbox = "danger-full-access".into()))
            .contains("danger-full-access is refused"));
        assert!(bad(Box::new(|o| {
            o.native_effort = "high".into();
            o.effort = "low".into();
        }))
        .contains("exclude each other"));
        assert!(bad(Box::new(|o| {
            o.thread = "abc".into();
            o.mode = "new".into();
        }))
        .contains("-Thread needs -Mode fork or resume"));
        assert!(bad(Box::new(|o| {
            o.range = "HEAD".into();
            o.purpose = "diff-review".into();
        }))
        .contains("two-point range"));
        assert!(bad(Box::new(|o| o.range = "a..b".into())).contains("-Range goes with -Purpose"));
        assert!(bad(Box::new(|o| o.format_retry = 2)).contains("-FormatRetry must be 0 or 1"));
        assert!(bad(Box::new(|o| {
            o.raw = true;
            o.schema_transport = "prompt-only".into();
        }))
        .contains("-SchemaTransport does not apply to -Raw"));
        assert!(bad(Box::new(
            |o| o.codex_config = vec!["model_provider=x".into()]
        ))
        .contains("part of the reviewer identity"));
        assert!(bad(Box::new(|o| o.max_model_steps = -3)).contains("must be a positive integer"));
        assert!(bad(Box::new(|o| o.panel = true)).contains("milestone 4"));
    }

    #[test]
    fn range_pair_detection() {
        assert!(is_range_pair("a..b"));
        assert!(is_range_pair("a...b"));
        assert!(!is_range_pair("HEAD"));
        assert!(!is_range_pair("a.."));
    }
}
