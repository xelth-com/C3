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
    /// `-PanelConcurrency` (0 = no cap; 1 = strictly sequential; k = at most k at once).
    pub panel_concurrency: i64,
    pub panel_concurrency_given: bool,
    /// `-PanelSize` (0 = the purpose default).
    pub panel_size: i64,
    pub panel_size_given: bool,
    /// `-PanelOrder` (`routed` | `roster`; empty = the default `routed`).
    pub panel_order: String,
    /// `-PanelSeed` (the draw nonce; empty = env/date).
    pub panel_seed: String,
    /// `-Require` matchers (repeatable; `none` alone drops the roster requirement).
    pub require: Vec<String>,
    /// `-Role` (one role for every member).
    pub role: String,
    /// `-Roles` (roles assigned by score rank and willingness; repeatable).
    pub roles: Vec<String>,
    /// `-Topic` tags (repeatable; used by routing and rating).
    pub topic: Vec<String>,
    /// `-PanelSpec` (INTERNAL): the base64 member spec a panel run hands each seat.
    pub panel_spec: String,
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

    // Panel surface (`codex-consult.ps1:1803-1823` and the `-Panel` branch 1971-1974). `-Panel`
    // seats roster reviewers, so it takes neither a single reviewer (`-Provider`) nor a thread;
    // its members always fork their own lineage's newest thread. The panel-only options refuse
    // outside a panel; the sizes and the order/seed are validated here (self-contained).
    let panel_run = o.panel || o.panel_all;
    if panel_run {
        if !o.provider.is_empty() {
            return Err("-Panel seats reviewers of the roster and does not take -Provider (for one reviewer, drop -Panel; to insist on one in the panel: -Require).".into());
        }
        if !o.thread.is_empty() {
            return Err("-Panel does not take -Thread: each member forks the newest thread of its own lineage (or starts one).".into());
        }
        if o.mode == "resume" {
            return Err("-Panel does not take -Mode resume: each member forks the newest thread of its own lineage (or starts one); -Mode new starts fresh threads for all.".into());
        }
    }
    if o.panel_concurrency_given && !panel_run {
        return Err("-PanelConcurrency goes with -Panel (or -PanelAll) only.".into());
    }
    if o.panel_concurrency < 0 {
        return Err(format!(
            "-PanelConcurrency must be 0 (no cap) or a positive number (got {}).",
            o.panel_concurrency
        ));
    }
    if !panel_run {
        if o.panel_size_given {
            return Err("-PanelSize goes with -Panel (or -PanelAll) only.".into());
        }
        if !o.panel_order.is_empty() {
            return Err("-PanelOrder goes with -Panel (or -PanelAll) only.".into());
        }
        if !o.panel_seed.is_empty() {
            return Err("-PanelSeed goes with -Panel (or -PanelAll) only.".into());
        }
        if !o.roles.is_empty() {
            return Err("-Roles goes with -Panel (or -PanelAll) only.".into());
        }
    }
    if o.panel_size_given {
        if o.panel_all {
            return Err("-PanelSize does not go with -PanelAll: -PanelAll runs every eligible member (drop one of them).".into());
        }
        if o.panel_size < 1 {
            return Err(format!(
                "-PanelSize must be 1 or more (got {}); leave it out for the purpose's size.",
                o.panel_size
            ));
        }
    }
    let panel_order = o.panel_order.trim().to_lowercase();
    if !panel_order.is_empty() && !["roster", "routed"].contains(&panel_order.as_str()) {
        return Err(format!(
            "-PanelOrder must be roster or routed (got '{panel_order}')."
        ));
    }
    let panel_seed = o.panel_seed.trim();
    if !panel_seed.is_empty() && !is_panel_seed(panel_seed) {
        return Err(format!(
            "-PanelSeed must be a number or a token (letters, digits, dot, dash, underscore, colon; got '{panel_seed}')."
        ));
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

/// A `-PanelSeed` token (`^[A-Za-z0-9][A-Za-z0-9._:-]{0,63}$`).
fn is_panel_seed(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    if !bytes[0].is_ascii_alphanumeric() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|&c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b':' | b'-'))
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
        // Panel combos (self-contained refusals).
        assert!(bad(Box::new(|o| {
            o.panel = true;
            o.provider = "openai".into();
        }))
        .contains("does not take -Provider"));
        assert!(bad(Box::new(|o| {
            o.panel = true;
            o.thread = "abc".into();
        }))
        .contains("-Panel does not take -Thread"));
        assert!(bad(Box::new(|o| {
            o.panel = true;
            o.mode = "resume".into();
        }))
        .contains("does not take -Mode resume"));
        assert!(bad(Box::new(|o| {
            o.panel = true;
            o.panel_all = true;
            o.panel_size = 3;
            o.panel_size_given = true;
        }))
        .contains("does not go with -PanelAll"));
        assert!(bad(Box::new(|o| {
            o.panel = true;
            o.panel_size = 0;
            o.panel_size_given = true;
        }))
        .contains("-PanelSize must be 1 or more"));
        assert!(bad(Box::new(|o| {
            o.panel = true;
            o.panel_order = "sideways".into();
        }))
        .contains("-PanelOrder must be roster or routed"));
        assert!(bad(Box::new(|o| {
            o.panel_size = 2;
            o.panel_size_given = true;
        }))
        .contains("-PanelSize goes with -Panel"));
        assert!(bad(Box::new(|o| {
            o.panel = true;
            o.panel_concurrency = -2;
            o.panel_concurrency_given = true;
        }))
        .contains("-PanelConcurrency must be 0"));
    }

    #[test]
    fn panel_defaults_pass() {
        let mut o = ok_opts();
        o.panel = true;
        assert!(validate(&o, None).is_ok());
    }

    #[test]
    fn range_pair_detection() {
        assert!(is_range_pair("a..b"));
        assert!(is_range_pair("a...b"));
        assert!(!is_range_pair("HEAD"));
        assert!(!is_range_pair("a.."));
    }
}
