//! The **read-only tree / collab drift check** of a `c3 consult` run: after a turn, decide whether
//! the working tree changed and, if so, whether the change can be the reviewer's (write-disabled
//! engines WARN, agy FAILS). Driven by [`super::finish`] via [`engine_tree_check`]; the outcome is
//! computed by [`engine_tree_outcome`]. Pure decision logic over two collab snapshots and a
//! [`super::Drift`], so it is unit-testable without a process.

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
/// usable). agy WARNS when its event stream shows at least one tool call and none could write/run
/// commands; it FAILS (`outcome = "failed"`, `problem` set, forced class `permission`) when the
/// stream shows a writing tool call or names no tool call at all. `outcome = "clean"` when nothing
/// changed.
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

pub(super) fn engine_tree_check(
    ctx: &Context,
    drift: &Drift,
    collab_before: &std::collections::HashMap<String, String>,
    collab_after: &std::collections::HashMap<String, String>,
    agy_tool_names: &[String],
    claude_init: &ClaudeInit,
) -> EngineTreeCheck {
    let mut why: Vec<String> = Vec::new();
    let mut warn: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    // The per-engine suffix on each warning line (the change is not the reviewer's).
    let not_mine = if ctx.engine == "agy" {
        format!(
            "agy called no writing tool ({}), the change is not the reviewer's",
            agy_tool_names.join(", ")
        )
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
        files.extend(drift.tree_changed_paths.iter().cloned());
        let cut = cut_files(&drift.tree_changed_paths);
        why.push(format!(
            "the working tree changed during the run (by the reviewer or anyone else): {cut}"
        ));
        warn.push(format!(
            "the working tree changed during the run ({cut}) - {not_mine}"
        ));
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
    let collab_changed = crate::engines::tree_check::compare_collab(
        collab_before,
        collab_after,
        ctx.task.as_str(),
        &own_prefixes,
    );
    if !collab_changed.is_empty() {
        let shown = collab_shown(ctx);
        let names: Vec<String> = collab_changed
            .iter()
            .map(|n| format!("{shown}{n}"))
            .collect();
        files.extend(names.iter().cloned());
        let cut = cut_files(&names);
        why.push(format!(
            "the collab directory changed during the run (by the reviewer or anyone else): {cut}"
        ));
        warn.push(format!(
            "the collab directory changed during the run ({cut}) - {not_mine}"
        ));
    }
    if drift.brief_changed {
        files.push("brief".to_string());
        why.push("the brief changed during the run (by the reviewer or anyone else)".to_string());
        warn.push(format!("the brief changed during the run - {not_mine}"));
    }
    if !drift.artifacts_changed_paths.is_empty() {
        files.extend(drift.artifacts_changed_paths.iter().cloned());
        let list = drift.artifacts_changed_paths.join(", ");
        why.push(format!(
            "artifact(s) changed during the run (by the reviewer or anyone else): {list}"
        ));
        warn.push(format!(
            "artifact(s) changed during the run ({list}) - {not_mine}"
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
    engine_tree_outcome(
        &ctx.engine,
        agy_tool_names,
        write_disabled,
        claude_note.as_deref(),
        why,
        warn,
        files,
    )
}

/// Decide the tree-check outcome from the collected change reasons (`why`), their warning form
/// (`warn`) and the changed paths (`files`). Pulled out of [`engine_tree_check`] so the fail/warn
/// rules are unit-testable without building a whole `Context`:
///   * nothing changed → `clean`;
///   * a write-disabled engine (muse/grok/http/claude) → `warned` (D9, decision 1);
///   * agy enforces no read-only sandbox, so its decision turns on the tool calls its own event
///     stream recorded (D9 / TECH_DEBT fourth instance): only read-only tool calls → `warned`; a
///     writing/command tool call → `failed` naming it; no tool call at all → `failed` reporting
///     that (decision 3);
///   * any other engine → `failed`.
pub(super) fn engine_tree_outcome(
    engine: &str,
    agy_tool_names: &[String],
    write_disabled: bool,
    note_override: Option<&str>,
    why: Vec<String>,
    warn: Vec<String>,
    files: Vec<String>,
) -> EngineTreeCheck {
    let agy_writing: Vec<String> = agy_tool_names
        .iter()
        .filter(|t| crate::engines::tree_check::agy_tool_is_writing(t))
        .cloned()
        .collect();
    // (BLOCKER, Muse review-2026-10-30) A writing tool call by agy FAILS the run even when the
    // monitored compare is clean: agy's write may have landed in a path the compare ignores (a
    // sibling's handoff, this task's stores, an untracked scratch file), and a write that the check
    // cannot attribute to someone else must not pass. Decided BEFORE the "nothing changed" early
    // return below.
    if engine == "agy" && !agy_writing.is_empty() {
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
    let agy_no_tools = agy_tool_names.is_empty();
    let agy_warns = engine == "agy" && !agy_no_tools && agy_writing.is_empty();
    if write_disabled || agy_warns {
        EngineTreeCheck {
            outcome: "warned".into(),
            problem: String::new(),
            warnings: warn,
            files,
        }
    } else {
        // FAIL. A caller-supplied note (claude's init proof) wins; else, for agy, name why the
        // stream did not clear it: a writing tool call, or no tool call at all (the remedy of the
        // TECH_DEBT fourth instance).
        let note = if let Some(n) = note_override {
            n.to_string()
        } else if engine == "agy" {
            if agy_no_tools {
                format!(
                    "{} (its event stream named no tool call)",
                    engine_tree_note(engine)
                )
            } else {
                format!(
                    "{} (a writing tool was called: {})",
                    engine_tree_note(engine),
                    agy_writing.join(", ")
                )
            }
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
