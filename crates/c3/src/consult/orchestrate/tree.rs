//! The **read-only tree / collab drift check** of a `c3 consult` run: after a turn, decide whether
//! the working tree changed and, if so, whether the change can be the reviewer's (write-disabled
//! engines WARN, agy FAILS). Driven by [`super::finish`] via [`engine_tree_check`]; the outcome is
//! computed by [`engine_tree_outcome`]. Pure decision logic over two collab snapshots and a
//! [`super::Drift`], so it is unit-testable without a process. (B1) A reviewer that ran in a
//! throwaway worktree is judged on that worktree's own changes; the live tree only warns
//! ([`with_live_tree_warnings`]).

use super::*;

/// The per-engine `TreeNote` appended to the tree-check problem (`EngineSpec.TreeNote`).
pub(super) fn engine_tree_note(engine: &str) -> String {
    match engine {
        "muse" => {
            "muse ran with --disable-write --disable-shell (the check cannot tell who changed it)"
                .to_string()
        }
        "grok" => {
            "grok ran with the read-only tool allowlist (--disallowed-tools write,run_terminal_command,use_tool,search_tool) (the check cannot tell who changed it)"
                .to_string()
        }
        "http" => {
            "the http reviewer received only a pack and never touched the machine (the change is not the reviewer's)".to_string()
        }
        "claude" => {
            "claude ran with --restricted and read tools only, but managed settings and their hooks still apply".to_string()
        }
        other => format!("{other}'s sandbox does not block writes"),
    }
}

/// `.collab/`-style prefix prepended to the collab-relative changed names in the tree problem.
fn collab_shown(ctx: &Context) -> String {
    let rel = c3_core::paths::repo_relative(&ctx.repo_root, &ctx.collab_root)
        .unwrap_or_else(|| ctx.o.collab_dir.clone());
    if rel.ends_with('/') {
        rel
    } else {
        format!("{rel}/")
    }
}

/// `<N> file(s): a, b, c, d, e, ...` (the plugin's `$cut`).
pub(super) fn cut_files(names: &[String]) -> String {
    let n = names.len();
    let mut list = names.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
    if n > 5 {
        list.push_str(", ...");
    }
    format!("{n} file{}: {list}", if n != 1 { "s" } else { "" })
}

/// Whether the engine ran write-disabled: its own tools cannot have made a change, so the tree
/// check WARNS instead of failing (D9).
fn engine_write_disabled(engine: &str) -> bool {
    // muse runs with --disable-write --disable-shell; grok runs with the read-only tool allowlist
    // (write/shell/MCP-gateway tools disallowed); claude runs with --restricted --tools
    // Read,Grep,Glob --permission-mode dontAsk (proven by each turn's init event) so its own tools
    // cannot have written; the http reviewer receives only a pack and never touches the machine at
    // all — so any tree change during such a run is not the reviewer's, and the check WARNS instead
    // of failing (D9). agy is NOT here: it enforces no read-only sandbox, so its fail/warn decision
    // turns on whether its event stream shows a writing tool call (see engine_tree_check).
    engine == "muse" || engine == "grok" || engine == "http" || engine == "claude"
}

/// The structured read-only tree check (`Get-EngineTreeCheck`, wave 26b D9). Compares the working
/// tree, the collab directory (this run's own handoff prefix, another task's directory, and this
/// task's control/ledger/state files ignored — see `compare_collab`), the brief and the artifacts.
/// A write-disabled engine (muse/grok/http/claude) WARNS (`outcome = "warned"`, the reply stays
/// usable). agy FAILS (`outcome = "failed"`, `problem` set, forced class `permission`) when its own
/// tool log shows a call that may have written (A1: decided per step from the log, see
/// [`crate::engines::tree_check::agy_tool_step_is_writing`]); otherwise a change WARNS when the
/// reply is complete and schema-valid ("reply kept", A1) or the stream shows at least one tool call,
/// and FAILS only when the stream names no tool call AND the reply is not complete.
/// `outcome = "clean"` when nothing changed.
pub(super) struct EngineTreeCheck {
    pub(super) outcome: String,
    pub(super) problem: String,
    pub(super) warnings: Vec<String>,
    pub(super) files: Vec<String>,
}

/// What a claude turn's init event proves for the tree decision (Muse, review-2026-10-30). The init
/// event names the tool set the CLI actually ran with; `--restricted --tools Read,Grep,Glob` is only
/// trusted when the init confirms it.
#[derive(Debug, Clone, Default)]
pub(super) struct ClaudeInit {
    /// No init event at all - the tool set could not be confirmed (keep WARN, add a note).
    pub(super) no_init: bool,
    /// Tools the init event listed that are outside the read-only allow-list (empty when confirmed
    /// read-only, or when this is not a claude run). Non-empty ⟹ NOT write-disabled (fails like agy
    /// on a tree change).
    pub(super) extra_tools: Vec<String>,
}

/// (A1) An agy run's tool log over every turn: the tool names in order (for the messages; empty =
/// the stream named no tool call) and the steps that may have written, as decided from each step's
/// own log entry. Empty for every other engine.
#[derive(Debug, Clone, Default)]
pub(super) struct AgyToolLog {
    pub(super) names: Vec<String>,
    pub(super) writing: Vec<String>,
}

/// The distinct entries of `names` in first-seen order (an ACTIVE and a DONE event of one step, and
/// hundreds of `read_file` calls, are named once).
fn distinct(names: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for n in names {
        if !out.contains(n) {
            out.push(n.clone());
        }
    }
    out
}

/// The collected change reasons of a tree check: `why` (the failure form), `warn` (the warning form)
/// and the changed paths.
pub(super) struct TreeChanges {
    pub(super) why: Vec<String>,
    pub(super) warn: Vec<String>,
    pub(super) files: Vec<String>,
}

/// (B1) The suffix of a live-tree change warning when the reviewer ran in a throwaway worktree.
pub(super) const NOT_THE_WORKTREE: &str =
    "the reviewer ran in a throwaway worktree, the change is not the reviewer's";

/// (B1) The check of a reviewer that ran in a throwaway worktree, from the outcome over the
/// worktree's own changes ([`engine_tree_outcome`]) and the live tree's changes during the run
/// (`main_warn`, already worded as warnings): those never fail the run - a clean worktree with a
/// changed live tree is `warned`, and the live-tree warnings are added to any outcome.
pub(super) fn with_live_tree_warnings(
    mut check: EngineTreeCheck,
    main_warn: Vec<String>,
) -> EngineTreeCheck {
    if main_warn.is_empty() {
        return check;
    }
    if check.outcome == "clean" {
        check.outcome = "warned".into();
    }
    check.warnings.extend(main_warn);
    check
}

/// The tree check over the turns of the run. (B1) `worktree_changes` is `Some` when the reviewer
/// ran in a throwaway worktree: its own changes there are what the outcome is decided on, and every
/// change of the live tree, its collab directory, the brief or an artifact is only a warning (a
/// parallel session editing the checkout is not the reviewer).
#[allow(clippy::too_many_arguments)]
pub(super) fn engine_tree_check(
    ctx: &Context,
    drift: &Drift,
    collab_before: &std::collections::HashMap<String, String>,
    collab_after: &std::collections::HashMap<String, String>,
    agy: &AgyToolLog,
    claude_init: &ClaudeInit,
    reply_complete: bool,
    worktree_changes: Option<&[String]>,
) -> EngineTreeCheck {
    let mut why: Vec<String> = Vec::new();
    let mut warn: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    // (B1) the live tree's changes of a reviewer in a throwaway worktree: warnings only.
    let isolated = worktree_changes.is_some();
    let mut main_warn: Vec<String> = Vec::new();
    // The per-engine suffix on each warning line (the change is not the reviewer's).
    let not_mine = if ctx.engine == "agy" {
        let names = distinct(&agy.names);
        // (A1) a complete, schema-valid reply is kept whatever changed around it.
        let kept = if reply_complete { " - reply kept" } else { "" };
        // (F11-1) agy runs without a write sandbox: a change its tool log does not explain is
        // UNATTRIBUTED (a write can be missing from the log), never "someone else's".
        if names.is_empty() {
            // only reached as a warning when the reply is complete and schema-valid.
            format!("agy's event stream named no tool call, the change is unattributed{kept}")
        } else {
            format!(
                "agy's tool log shows no writing step ({}), the change is unattributed{kept}",
                names.join(", ")
            )
        }
    } else if ctx.engine == "claude" {
        "claude ran with --restricted and read tools only, the change is not the reviewer's"
            .to_string()
    } else {
        format!(
            "{} ran write-disabled, the change is not the reviewer's",
            ctx.engine
        )
    };
    if drift.tree_changed {
        let cut = cut_files(&drift.tree_changed_paths);
        if isolated {
            main_warn.push(format!(
                "the main working tree changed during the run ({cut}) - {NOT_THE_WORKTREE}"
            ));
        } else {
            files.extend(drift.tree_changed_paths.iter().cloned());
            why.push(format!(
                "the working tree changed during the run (by the reviewer or anyone else): {cut}"
            ));
            warn.push(format!(
                "the working tree changed during the run ({cut}) - {not_mine}"
            ));
        }
    }
    let own = format!(
        "{}/handoffs/{:02}-{}-{}.",
        ctx.task, ctx.nn, ctx.file_prefix, ctx.reply_name
    );
    // A member of a panel whose members run at the same time (D7): the task's two stores and the
    // siblings' handoffs are theirs to write meanwhile (each with its atomic-write temp); every
    // other collab path stays monitored.
    let mut own_prefixes = vec![own];
    if let Some(m) = &ctx.panel_member {
        own_prefixes.extend(crate::engines::tree_check::panel_ignore_prefixes(
            ctx.task.as_str(),
            &m.sibling_nns,
        ));
    }
    let mut collab_changed = crate::engines::tree_check::compare_collab(
        collab_before,
        collab_after,
        ctx.task.as_str(),
        &own_prefixes,
    );
    // (R5, F18-3; 0.2.5 D1, F27-3) the bridge's own `--diff` patch is the run's own file - exactly
    // the file it wrote and that file's write-temp name (a panel sibling that writes the same patch
    // included), nothing else that shares the patch's stem
    if let Some(d) = &ctx.diff {
        collab_changed
            .retain(|k| !crate::consult::reviewed::is_run_patch_file(d, ctx.task.as_str(), k));
    }
    if !collab_changed.is_empty() {
        let shown = collab_shown(ctx);
        let names: Vec<String> = collab_changed
            .iter()
            .map(|n| format!("{shown}{n}"))
            .collect();
        let cut = cut_files(&names);
        if isolated {
            main_warn.push(format!(
                "the live collab directory changed during the run ({cut}) - {NOT_THE_WORKTREE}"
            ));
        } else {
            files.extend(names.iter().cloned());
            why.push(format!(
                "the collab directory changed during the run (by the reviewer or anyone else): {cut}"
            ));
            warn.push(format!(
                "the collab directory changed during the run ({cut}) - {not_mine}"
            ));
        }
    }
    // (F11-2) a reviewer of the LIVE tree reads the live brief and artifacts while they may change:
    // a change of a BOUND input is not ambient drift - the reply may answer other bytes than the
    // ones the review is bound to - so it is collected apart and withholds the reply
    // ([`with_bound_input_change`]). (0.2.5 D1) Every engine that reads the live tree, not agy only.
    let bound_apart = bound_inputs_withhold(&ctx.engine, isolated);
    let bound_changed: Vec<String> = if bound_apart {
        bound_inputs_changed(&ctx.brief_ref, drift)
    } else {
        Vec::new()
    };
    if drift.brief_changed {
        if isolated {
            main_warn.push(format!(
                "the brief changed during the run (the reviewer read the copy staged at launch) - {NOT_THE_WORKTREE}"
            ));
        } else if !bound_apart {
            files.push("brief".to_string());
            why.push(
                "the brief changed during the run (by the reviewer or anyone else)".to_string(),
            );
            warn.push(format!("the brief changed during the run - {not_mine}"));
        }
    }
    if !drift.artifacts_changed_paths.is_empty() {
        let list = drift.artifacts_changed_paths.join(", ");
        if isolated {
            main_warn.push(format!(
                "artifact(s) changed during the run ({list}) - {NOT_THE_WORKTREE}"
            ));
        } else if !bound_apart {
            files.extend(drift.artifacts_changed_paths.iter().cloned());
            why.push(format!(
                "artifact(s) changed during the run (by the reviewer or anyone else): {list}"
            ));
            warn.push(format!(
                "artifact(s) changed during the run ({list}) - {not_mine}"
            ));
        }
    }
    // (B1) the reviewer's own writes in its throwaway worktree: only its process tree runs there,
    // so this - not the live tree - is what the outcome turns on.
    if let Some(wt) = worktree_changes.filter(|w| !w.is_empty()) {
        files.extend(wt.iter().cloned());
        let cut = cut_files(wt);
        why.push(format!(
            "the reviewer's throwaway worktree changed during the run: {cut}"
        ));
        warn.push(format!(
            "the reviewer's throwaway worktree changed during the run ({cut}) - {not_mine}"
        ));
    }
    // (Muse, review-2026-10-30) The claude tree decision turns on its init proof, not on the engine
    // name: its own tools cannot have written ONLY when the init event confirms the read-only tool
    // set. A claude turn whose init lists a tool outside Read/Grep/Glob is NOT write-disabled (a
    // tree change then FAILS like agy); a turn with NO init event stays write-disabled (WARN) but
    // the tool set could not be confirmed, so a note says so.
    let claude_write_capable = ctx.engine == "claude" && !claude_init.extra_tools.is_empty();
    if ctx.engine == "claude" && claude_init.no_init {
        warn.push("no init event: the tool set could not be confirmed".to_string());
    }
    let write_disabled = if ctx.engine == "claude" {
        !claude_write_capable
    } else {
        engine_write_disabled(&ctx.engine)
    };
    let claude_note = claude_write_capable.then(|| {
        format!(
            "claude's init event listed tool(s) outside {}: {} - its read-only capability is not proven",
            c3_core::claude::CLAUDE_TOOLS_ARG,
            claude_init.extra_tools.join(", ")
        )
    });
    let check = engine_tree_outcome(
        &ctx.engine,
        agy,
        write_disabled,
        reply_complete,
        claude_note.as_deref(),
        TreeChanges { why, warn, files },
    );
    with_live_tree_warnings(with_bound_input_change(check, &bound_changed), main_warn)
}

/// (F11-2, 0.2.5 D1) Whether a change of a BOUND input (the brief, an artifact) withholds the reply
/// of a run of `engine`: every engine whose reviewer reads the LIVE tree - agy, muse, grok, claude,
/// kimi and codex - when it did not run in a throwaway worktree (`isolated`: its inputs are the
/// copies staged at launch, so a live change does not reach it). The http reviewer reads no file at
/// all: it received a pack built before the request.
pub(super) fn bound_inputs_withhold(engine: &str, isolated: bool) -> bool {
    !isolated && engine != "http"
}

/// (F11-2, 0.2.5 D1) The bound inputs that changed during the run, as the problem names them: the
/// brief (with its reference, when it has one), then each changed artifact.
pub(super) fn bound_inputs_changed(brief_ref: &str, drift: &Drift) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if drift.brief_changed {
        out.push(if brief_ref.is_empty() {
            "the brief".to_string()
        } else {
            format!("the brief `{brief_ref}`")
        });
    }
    out.extend(
        drift
            .artifacts_changed_paths
            .iter()
            .map(|p| format!("the artifact `{p}`")),
    );
    out
}

/// (F11-2) A BOUND input - the brief, an artifact - that changed while a live-tree reviewer read it: the outcome is
/// `failed` (the reply is not recorded as usable; the finish keeps its raw text, and the forced
/// class is the one of a failed read-only check, `permission`), and the problem names the changed
/// input(s). Ambient drift (the tree, the collab directory) stays a warning. `check` unchanged when
/// no bound input changed.
pub(super) fn with_bound_input_change(
    mut check: EngineTreeCheck,
    bound_changed: &[String],
) -> EngineTreeCheck {
    if bound_changed.is_empty() {
        return check;
    }
    let list = bound_changed.join(", ");
    let problem = format!(
        "a bound input changed during the run: {list} - the reply may answer other bytes than the review is bound to, so it is not recorded as usable"
    );
    check.files.extend(bound_changed.iter().cloned());
    if check.outcome == "failed" && !check.problem.is_empty() {
        check.problem = format!("{}; {problem}", check.problem);
    } else {
        check.outcome = "failed".into();
        check.problem = problem;
    }
    check
}

/// Decide the tree-check outcome from the collected change reasons (`why`), their warning form
/// (`warn`) and the changed paths (`files`). Pulled out of [`engine_tree_check`] so the fail/warn
/// rules are unit-testable without building a whole `Context`:
///   * agy whose own tool log shows a call that may have written → `failed` naming it, even when
///     nothing monitored changed (its write may have landed in an ignored path);
///   * nothing changed → `clean`;
///   * a write-disabled engine (muse/grok/http/claude) → `warned` (D9, decision 1);
///   * agy enforces no read-only sandbox, so its decision turns on its own tool log: no writing call
///     and a complete, schema-valid reply (`reply_complete`) → `warned`, the reply kept (A1: a
///     failure is "no usable reply", never a change somebody else made); no writing call and at
///     least one tool call → `warned`; no tool call at all and no complete reply → `failed`
///     reporting that (decision 3);
///   * any other engine → `failed`.
pub(super) fn engine_tree_outcome(
    engine: &str,
    agy: &AgyToolLog,
    write_disabled: bool,
    reply_complete: bool,
    note_override: Option<&str>,
    changes: TreeChanges,
) -> EngineTreeCheck {
    let TreeChanges { why, warn, files } = changes;
    let agy_writing = if engine == "agy" {
        distinct(&agy.writing)
    } else {
        Vec::new()
    };
    // (BLOCKER, Muse review-2026-10-30) A writing tool call by agy FAILS the run even when the
    // monitored compare is clean: agy's write may have landed in a path the compare ignores (a
    // sibling's handoff, this task's stores, an untracked scratch file), and a write that the check
    // cannot attribute to someone else must not pass. Decided BEFORE the "nothing changed" early
    // return below.
    if !agy_writing.is_empty() {
        let why = if why.is_empty() {
            vec![format!(
                "agy called a writing tool during the run ({}); its write may fall outside the monitored paths",
                agy_writing.join(", ")
            )]
        } else {
            why
        };
        return EngineTreeCheck {
            outcome: "failed".into(),
            problem: format!(
                "{} - {} (a writing tool was called: {})",
                why.join("; "),
                engine_tree_note(engine),
                agy_writing.join(", ")
            ),
            warnings: Vec::new(),
            files,
        };
    }
    if why.is_empty() {
        return EngineTreeCheck {
            outcome: "clean".into(),
            problem: String::new(),
            warnings: Vec::new(),
            files,
        };
    }
    let agy_no_tools = agy.names.is_empty();
    let agy_warns = engine == "agy" && (reply_complete || !agy_no_tools);
    if write_disabled || agy_warns {
        EngineTreeCheck {
            outcome: "warned".into(),
            problem: String::new(),
            warnings: warn,
            files,
        }
    } else {
        // FAIL. A caller-supplied note (claude's init proof) wins; else, for agy (no tool call and
        // no complete reply), say the stream named no tool call.
        let note = if let Some(n) = note_override {
            n.to_string()
        } else if engine == "agy" {
            format!(
                "{} (its event stream named no tool call)",
                engine_tree_note(engine)
            )
        } else {
            engine_tree_note(engine)
        };
        EngineTreeCheck {
            outcome: "failed".into(),
            problem: format!("{} - {}", why.join("; "), note),
            warnings: Vec::new(),
            files,
        }
    }
}
