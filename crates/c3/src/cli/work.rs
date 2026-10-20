//! `c3 work` — one worker run (`docs/work-mode.md`): a roster model as an ordinary
//! coding worker. The flags map one-to-one onto [`crate::work::Options`]; the guards
//! (roster selection, prompt cap, allow list) live in [`crate::work`].

use clap::Args;

use crate::work::Options;

/// Arguments of `c3 work`. `--task` is required by every form; `--brief`/`--provider`
/// by a run (`--status`/`--wait` read the records only, `--detach-id` is internal).
#[derive(Args, Debug)]
pub struct WorkArgs {
    /// The task slug (letters, digits, dot, dash, underscore): the collab dir's task.
    #[arg(long, default_value = "")]
    pub task: String,
    /// The brief file (repo-relative or absolute); c3 never writes briefs.
    #[arg(long, default_value = "")]
    pub brief: String,
    /// The roster label of the worker (engine claude, role "worker").
    #[arg(long, default_value = "")]
    pub provider: String,
    /// The model id ("" = the roster entry's model, same endpoint).
    #[arg(long, default_value = "")]
    pub model: String,
    /// Extra ask text appended to the prompt after the brief.
    #[arg(long, default_value = "")]
    pub prompt: String,
    /// low | medium | high | xhigh ("" = send none).
    #[arg(long, default_value = "")]
    pub effort: String,
    /// The turn cap (0 = 80).
    #[arg(long, default_value_t = 0)]
    pub max_turns: i64,
    /// The run timeout in seconds (0 = 3600).
    #[arg(long, default_value_t = 0)]
    pub timeout_sec: i64,
    /// A Claude Code --allowedTools pattern (repeatable; "" list = the cargo defaults).
    /// A pattern naming git is refused: a worker never commits.
    #[arg(long = "allow", value_name = "PATTERN")]
    pub allow: Vec<String>,
    /// Where work runs are stored; a relative path resolves against the repo root.
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,
    /// Start the run in the background and print the come-back commands.
    #[arg(long)]
    pub detach: bool,
    /// Print the work runs of the task (exit 0 done, 1 died/timed out, 2 running).
    #[arg(long)]
    pub status: bool,
    /// Wait until the task's work runs are done, then print as --status.
    #[arg(long)]
    pub wait: bool,
    /// The work run id (or its beginning) --status/--wait narrows to.
    #[arg(long, default_value = "")]
    pub id: String,
    /// --wait's timeout in seconds (0 = the run's own budget).
    #[arg(long)]
    pub wait_timeout_sec: Option<i64>,
    /// INTERNAL: the background process of --detach; never pass it yourself.
    #[arg(long, default_value = "", hide = true)]
    pub detach_id: String,
}

pub fn run(args: WorkArgs) -> i32 {
    crate::work::run(Options {
        task: args.task,
        brief: args.brief,
        provider: args.provider,
        model: args.model,
        prompt: args.prompt,
        effort: args.effort,
        max_turns: args.max_turns,
        timeout_sec: args.timeout_sec,
        allow: args.allow,
        collab_dir: args.collab_dir,
        detach: args.detach,
        status: args.status,
        wait: args.wait,
        id: args.id,
        wait_timeout_sec: args.wait_timeout_sec.unwrap_or(0),
        wait_timeout_sec_given: args.wait_timeout_sec.is_some(),
        detach_id: args.detach_id,
    })
}
