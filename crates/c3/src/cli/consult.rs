//! `c3 consult` — one consultation (milestone 2). The argument surface mirrors
//! `codex-consult.ps1` (see `docs/port/cli-surface.md`); the flow lives in
//! [`crate::consult`] and the engine adapters in [`crate::engines`].

use clap::Args;

use crate::consult::args::Options;

/// Arguments of `c3 consult` (see `docs/port/cli-surface.md`). A PowerShell `-PascalCase`
/// parameter becomes a `--kebab-case` flag; `[string[]]` parameters are natively repeatable.
#[derive(Args, Debug, Default)]
pub struct ConsultArgs {
    /// The task slug (letters, digits, dot, dash, underscore). Required by every form but
    /// `--explain` (refused with the plugin's text when missing, exit 1).
    #[arg(long, default_value = "")]
    pub task: String,
    /// (wave 27, R13 D5) `coordinate | consult | providers`: print that skill of the plugin for a
    /// host without skills (`-Explain`); takes no other parameter.
    #[arg(long)]
    pub explain: Option<String>,
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
    /// The stall cut in seconds without an event (0 = off; -1 = the roster entry's stall_sec,
    /// else 900). `allow_hyphen_values` so a negative value reaches the validator.
    #[arg(long, allow_hyphen_values = true, default_value_t = -1)]
    pub stall_sec: i64,
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
    /// (--engine http) The environment variable the API key is read from (default:
    /// OPENROUTER_API_KEY). The key value itself is never a flag.
    #[arg(long, default_value = "")]
    pub key_env: String,
    /// (--engine http) The API base URL (https:// only; default: OpenRouter's).
    #[arg(long, default_value = "")]
    pub base_url: String,
    /// (--engine http) The reviewer pack periphery budget in tokens (0..=200000; 0 = none).
    /// `allow_hyphen_values` so the -1 "not given" sentinel and a negative refusal reach the
    /// validator instead of clap rejecting it.
    #[arg(long, allow_hyphen_values = true, default_value_t = -1)]
    pub pack_budget: i64,
    /// (--engine http) A federation peer to bring into the reviewer pack (repeatable). Refused
    /// for codex/agy/muse, which read the repository through their own tools.
    #[arg(long)]
    pub peer: Vec<String>,
    /// (--engine http) `--peers all`: bring every peer this project may use in packs into the
    /// pack. Any other value is refused.
    #[arg(long)]
    pub peers: Option<String>,
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
    /// Telemetry for this run: on | off; it wins over CODEX_CONSULT_TELEMETRY (unset: on). Empty =
    /// the environment decides. A panel passes it to its members.
    #[arg(long, default_value = "")]
    pub telemetry: String,

    // --- parsed-but-refused (engine scope M2d / panel M4 / detach R12) ---
    /// codex | agy | muse | http.
    #[arg(long, default_value = "")]
    pub engine: String,
    /// A non-codex engine launcher path (M2d).
    #[arg(long, default_value = "")]
    pub engine_exe: String,
    /// agy denial retry (M2d).
    #[arg(long, default_value_t = 1)]
    pub denial_retry: i64,
    /// muse max model steps (M2d). `allow_hyphen_values` so a negative value reaches the
    /// validator (which refuses it) instead of clap rejecting `-3` as an unknown flag.
    #[arg(long, allow_hyphen_values = true, default_value_t = 0)]
    pub max_model_steps: i64,
    /// Run every available roster reviewer as a panel (needs a roster; not with
    /// --provider/--thread/--mode resume).
    #[arg(long)]
    pub panel: bool,
    /// Panel including weighty entries (forces size 0; not with --panel-size).
    #[arg(long)]
    pub panel_all: bool,
    /// Panel concurrency: 0 = no cap, 1 = strictly sequential, k = at most k at once.
    #[arg(long, allow_hyphen_values = true)]
    pub panel_concurrency: Option<i64>,
    /// The number of panel seats (0 = the purpose default; not with --panel-all).
    #[arg(long, allow_hyphen_values = true)]
    pub panel_size: Option<i64>,
    /// Seat order: routed (default, a seeded weighted draw) or roster.
    #[arg(long, default_value = "")]
    pub panel_order: String,
    /// The panel draw nonce (else CODEX_CONSULT_TEST_PANEL_SEED, else today's UTC date).
    #[arg(long, default_value = "")]
    pub panel_seed: String,
    /// A required reviewer matcher (#n, a label, or `<provider> :: <model> [engine]`);
    /// repeatable. `--require none` alone drops the roster's requirement.
    #[arg(long)]
    pub require: Vec<String>,
    /// One role assigned to every panel member (in the prompt after the ask).
    #[arg(long, default_value = "")]
    pub role: String,
    /// Roles assigned to panel seats by score rank and willingness (repeatable).
    #[arg(long)]
    pub roles: Vec<String>,
    /// A topic tag for routing/rating (repeatable).
    #[arg(long)]
    pub topic: Vec<String>,
    /// INTERNAL: the base64 member spec a panel run hands each seat's child process. Never
    /// passed by hand.
    #[arg(long, default_value = "", hide = true)]
    pub panel_spec: String,
    /// Run the consultation in a background process and return at once (R12).
    #[arg(long)]
    pub detach: bool,
    /// Print the detached runs of the task (R12).
    #[arg(long)]
    pub status: bool,
    /// The detach id (or its beginning) for --status/--wait (R12).
    #[arg(long, default_value = "")]
    pub id: String,
    /// INTERNAL: the background process of --detach; never pass it yourself (R12).
    #[arg(long, default_value = "", hide = true)]
    pub detach_id: String,
    /// Reserved (R12).
    #[arg(long)]
    pub list: bool,
    /// Wait until the detached run(s) of the task are done, then print as --status (R12).
    #[arg(long)]
    pub wait: bool,
    /// --wait's timeout in seconds (0 = the run's own budget) (R12). `Option` so an explicit
    /// `--wait-timeout-sec 0` is distinguishable from the default (it is refused).
    #[arg(long, allow_hyphen_values = true)]
    pub wait_timeout_sec: Option<i64>,
    /// With --status: delete the files of old done/died detached runs (R12).
    #[arg(long)]
    pub prune: bool,
    /// (wave 26b, D10) Stop one running member of the task by its handoff number (needs --member).
    #[arg(long)]
    pub kick: bool,
    /// (wave 26b, D10) The handoff number (NN) --kick acts on.
    #[arg(long, default_value = "")]
    pub member: String,
}

/// Run one consultation and return the process exit code.
pub fn run(args: ConsultArgs) -> i32 {
    // (wave 27, R13 D5) -Explain: the one form without -Task.
    if let Some(name) = &args.explain {
        let raw: Vec<String> = std::env::args().skip(2).collect();
        return crate::consult::explain::run(name, &crate::consult::explain::other_flags(&raw));
    }
    // Every other form needs -Task (the plugin refuses instead of prompting).
    if args.task.trim().is_empty() {
        eprintln!("codex-consult: -Task <id> is required (a slug: the task directory <CollabDir>/<id>/); the one form without it is -Explain coordinate|consult|providers.");
        return 1;
    }
    // (R17) the run's telemetry switch: on | off or nothing - refused before anything starts.
    let tele = args.telemetry.trim().to_ascii_lowercase();
    if !tele.is_empty() && tele != "on" && tele != "off" {
        println!("codex-consult: -Telemetry must be on or off (got '{tele}'); leave it out for CODEX_CONSULT_TELEMETRY (unset: on).");
        return 1;
    }
    // Whether the user actually passed --continue-sec (clap can't tell a default -1 from an
    // explicit -1; treat any value != -1 as given, and -1 as the default sentinel).
    let continue_sec_given = args.continue_sec != -1;
    let stall_sec_given = args.stall_sec != -1;
    let panel_concurrency_given = args.panel_concurrency.is_some();
    let panel_size_given = args.panel_size.is_some();
    let id_given = !args.id.is_empty();
    let wait_timeout_sec_given = args.wait_timeout_sec.is_some();
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
        stall_sec: args.stall_sec,
        stall_sec_given,
        range: args.range,
        reply_name: args.reply_name,
        artifacts: args.artifact,
        raw: args.raw,
        codex_exe: args.codex_exe,
        provider: args.provider,
        key_env: args.key_env,
        base_url: args.base_url,
        pack_budget: args.pack_budget,
        peer: args.peer,
        peers: args.peers,
        native_effort: args.native_effort,
        off_peak_only: args.off_peak_only,
        skip_preflight: args.skip_preflight,
        codex_config: args.codex_config,
        schema_transport: args.schema_transport,
        telemetry: match args.telemetry.trim().to_ascii_lowercase().as_str() {
            "on" => Some(true),
            "off" => Some(false),
            _ => None,
        },
        format_retry: args.format_retry,
        dry_run: args.dry_run,
        engine: args.engine,
        engine_exe: args.engine_exe,
        denial_retry: args.denial_retry,
        max_model_steps: args.max_model_steps,
        panel: args.panel,
        panel_all: args.panel_all,
        panel_concurrency: args.panel_concurrency.unwrap_or(0),
        panel_concurrency_given,
        panel_size: args.panel_size.unwrap_or(0),
        panel_size_given,
        panel_order: args.panel_order,
        panel_seed: args.panel_seed,
        require: args.require,
        role: args.role,
        roles: args.roles,
        topic: args.topic,
        panel_spec: args.panel_spec,
        detach: args.detach,
        status: args.status,
        id: args.id,
        id_given,
        detach_id: args.detach_id,
        list: args.list,
        wait: args.wait,
        wait_timeout_sec: args.wait_timeout_sec.unwrap_or(0),
        wait_timeout_sec_given,
        prune: args.prune,
        kick: args.kick,
        member: args.member,
    };
    crate::consult::run(opts)
}
