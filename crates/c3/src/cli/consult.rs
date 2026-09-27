//! `c3 consult` — one consultation (milestone 2). The argument surface mirrors
//! `codex-consult.ps1` (see `docs/port/cli-surface.md`); the flow lives in
//! [`crate::consult`] and the engine adapters in [`crate::engines`].

use clap::Args;

use crate::consult::args::Options;

/// Arguments of `c3 consult` (see `docs/port/cli-surface.md`). A PowerShell `-PascalCase`
/// parameter becomes a `--kebab-case` flag; `[string[]]` parameters are natively repeatable.
#[derive(Args, Debug, Default)]
pub struct ConsultArgs {
    /// The task slug (letters, digits, dot, dash, underscore).
    #[arg(long)]
    pub task: String,
    /// Where consultations are stored; a relative path resolves against the repo root.
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,
    /// new | fork | resume (empty = auto: fork the newest thread of this lineage, else new).
    #[arg(long, default_value = "")]
    pub mode: String,
    /// The thread (uuid) to fork or resume; needs --mode fork or resume.
    #[arg(long, default_value = "")]
    pub thread: String,
    /// The brief file (repo-relative or absolute); c3 never writes briefs.
    #[arg(long, default_value = "")]
    pub brief: String,
    /// The ask, prepended to the prompt.
    #[arg(long, default_value = "")]
    pub prompt: String,
    /// The model id.
    #[arg(long, default_value = "")]
    pub model: String,
    /// framing | decision | checkpoint | core-contract | acceptance | diff-review | stuck | chore.
    #[arg(long, default_value = "")]
    pub purpose: String,
    /// low | medium | high | xhigh (mapped through the endpoint's effort vocabulary).
    #[arg(long, default_value = "")]
    pub effort: String,
    /// read-only | workspace-write (danger-full-access is refused).
    #[arg(long, default_value = "")]
    pub sandbox: String,
    /// The reply_markdown word cap (0 = the purpose preset).
    #[arg(long, default_value_t = 0)]
    pub max_words: i64,
    /// The run timeout in seconds (0 = the purpose default).
    #[arg(long, default_value_t = 0)]
    pub timeout_sec: i64,
    /// The timeout-continuation budget (-1 = min(timeout, 900); 0 = no continuation).
    #[arg(long, default_value_t = -1)]
    pub continue_sec: i64,
    /// base..head or base...head (diff-review / acceptance only).
    #[arg(long, default_value = "")]
    pub range: String,
    /// The handoff base name (a slug); default `reply`.
    #[arg(long, default_value = "")]
    pub reply_name: String,
    /// A file bound to the review (repeatable).
    #[arg(long)]
    pub artifact: Vec<String>,
    /// A plain-text (0.1) consultation: no schema, no findings bookkeeping.
    #[arg(long)]
    pub raw: bool,
    /// Explicit path to the codex launcher (env override: CODEX_CONSULT_EXE).
    #[arg(long, default_value = "")]
    pub codex_exe: String,
    /// The provider label; needs --model.
    #[arg(long, default_value = "")]
    pub provider: String,
    /// An effort value sent verbatim (excludes --effort).
    #[arg(long, default_value = "")]
    pub native_effort: String,
    /// Refuse a run inside the provider's declared peak window.
    #[arg(long)]
    pub off_peak_only: bool,
    /// Skip the availability preflight (then warn).
    #[arg(long)]
    pub skip_preflight: bool,
    /// A per-run `-c key=value` override (repeatable); identity/effort keys are refused.
    #[arg(long)]
    pub codex_config: Vec<String>,
    /// output-schema | prompt-only (empty = the caps-v1 default for the endpoint).
    #[arg(long, default_value = "")]
    pub schema_transport: String,
    /// One format-repair turn if the reply is not valid JSON (0|1; default 1).
    #[arg(long, default_value_t = 1)]
    pub format_retry: i64,
    /// Print the plan and exit; a dry run writes nothing.
    #[arg(long)]
    pub dry_run: bool,

    // --- parsed-but-refused (engine scope M2d / panel M4 / detach R12) ---
    /// codex | agy | muse (only codex runs in M2c).
    #[arg(long, default_value = "")]
    pub engine: String,
    /// A non-codex engine launcher path (M2d).
    #[arg(long, default_value = "")]
    pub engine_exe: String,
    /// agy denial retry (M2d).
    #[arg(long, default_value_t = 1)]
    pub denial_retry: i64,
    /// muse max model steps (M2d).
    #[arg(long, default_value_t = 0)]
    pub max_model_steps: i64,
    /// Run every available roster reviewer (M4).
    #[arg(long)]
    pub panel: bool,
    /// Panel including weighty entries (M4).
    #[arg(long)]
    pub panel_all: bool,
    /// Reserved (R12).
    #[arg(long)]
    pub detach: bool,
    /// Reserved (R12).
    #[arg(long)]
    pub status: bool,
    /// Reserved (R12).
    #[arg(long)]
    pub list: bool,
    /// Reserved (R12).
    #[arg(long)]
    pub wait: bool,
    /// Reserved (R12).
    #[arg(long)]
    pub prune: bool,
}

/// Run one consultation and return the process exit code.
pub fn run(args: ConsultArgs) -> i32 {
    // Whether the user actually passed --continue-sec (clap can't tell a default -1 from an
    // explicit -1; treat any value != -1 as given, and -1 as the default sentinel).
    let continue_sec_given = args.continue_sec != -1;
    let denial_retry_given = args.denial_retry != 1;
    let opts = Options {
        task: args.task,
        collab_dir: args.collab_dir,
        mode: args.mode,
        thread: args.thread,
        brief: args.brief,
        prompt: args.prompt,
        model: args.model,
        purpose: args.purpose,
        effort: args.effort,
        sandbox: args.sandbox,
        max_words: args.max_words,
        timeout_sec: args.timeout_sec,
        continue_sec: args.continue_sec,
        continue_sec_given,
        range: args.range,
        reply_name: args.reply_name,
        artifacts: args.artifact,
        raw: args.raw,
        codex_exe: args.codex_exe,
        provider: args.provider,
        native_effort: args.native_effort,
        off_peak_only: args.off_peak_only,
        skip_preflight: args.skip_preflight,
        codex_config: args.codex_config,
        schema_transport: args.schema_transport,
        format_retry: args.format_retry,
        dry_run: args.dry_run,
        engine: args.engine,
        engine_exe: args.engine_exe,
        denial_retry_given,
        max_model_steps: args.max_model_steps,
        panel: args.panel,
        panel_all: args.panel_all,
        detach: args.detach,
        status: args.status,
        list: args.list,
        wait: args.wait,
        prune: args.prune,
    };
    crate::consult::run(opts)
}
