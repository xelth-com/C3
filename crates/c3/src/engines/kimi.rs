//! The `kimi` engine adapter: run one Moonshot Kimi CLI turn
//! (`kimi -p <prompt> --output-format stream-json -m <model>`) and turn its stream-json records
//! into an [`AttemptOutcome`].
//!
//! The argv is the plugin's (built by [`c3_core::engine::SubprocessEngine::plan_kimi`]); the prompt
//! rides in argv (`-p <prompt>`) and stdin stays empty (delivery [`PromptDelivery::PromptFile`],
//! like muse/grok, so the runner writes nothing to stdin). The stream-json records
//! ([`super::subprocess`] captured them to `.events.jsonl`) are parsed by [`read_kimi_events`] and
//! judged by [`kimi_turn_outcome`].
//!
//! The stream-json shape (kimi 0.27.0) is ROLE-based, not type-based:
//! - `{"role":"assistant","content":"<text>"}` - a final-answer chunk (concatenated into the reply);
//! - `{"role":"assistant","tool_calls":[{"function":{"name":"Read",...}},...]}` - tool call(s) the
//!   model issued (the evidence it read the code); a line may carry BOTH `content` (narration) and
//!   `tool_calls` - then the content is narration and is NOT part of the reply;
//! - `{"role":"tool","tool_call_id":"...","content":"..."}` - a tool result (ignored);
//! - `{"role":"meta","type":"session.resume_hint","session_id":"session_<uuid>",...}` - the LAST
//!   line of a clean run; the session id is READ from it (never minted) and resumed with `-S <id>`.
//!
//! kimi reports no token usage and has no stream-level error record: a start failure (bad model id,
//! not signed in, quota) comes on STDERR (`error: failed to run prompt: ...`) with a non-zero exit;
//! a benign libuv `Assertion failed: ...` shutdown line on stderr is ignored. kimi bills the Kimi
//! Code plan through a managed oauth sign-in; there is no per-token API-key billing route to guard
//! (unlike grok/muse), so the child inherits this process's environment (minus the host markers,
//! scrubbed by [`run_turn`]). kimi has no read-only sandbox flag in `-p` mode, so read-only is
//! requested in the prompt and enforced by the orchestrator's [`super::tree_check`] (any tree
//! change fails the run).

use std::path::{Path, PathBuf};

use c3_core::engine::{
    AttemptOutcome, Capabilities, ConversationId, ConversationTrust, Engine, EngineError,
    EngineKind, LaunchPlan, Mode, RawReply, Reply, Request, StructuredReply, SubprocessEngine,
    TurnKind, TurnRequest, KIMI_MAX_ARGV_PROMPT,
};
use c3_core::health::{convert_from_provider_error_text, provider_failure_class};
use serde_json::Value;

use super::codex::TurnFiles;
use super::subprocess::{run_turn, SpawnRequest, ToolFlight};

/// The runtime `kimi` engine: the resolved launcher, the working directory (repo root) and the
/// per-turn stream files. `plan`/`capabilities` delegate to the core [`SubprocessEngine`].
#[derive(Clone)]
pub struct KimiEngine {
    pub launcher: String,
    pub cwd: PathBuf,
    /// The primary turn's streams.
    pub primary: TurnFiles,
    /// The secondary turn's streams (timeout continuation).
    pub secondary: TurnFiles,
    /// The stall cut in seconds for the PRIMARY turn (`0` = off).
    pub stall_sec: i64,
    /// The consultation purpose (`acceptance`, `framing`, ...). The zero-tool-call review guard
    /// ([`kimi_review_tool_guard`]) reads it on the PRIMARY turn; empty / irrelevant for a secondary
    /// (timeout-continuation) turn.
    pub purpose: String,
    /// The operator's kick file for the PRIMARY turn (`None` = no kick watch).
    pub kick_path: Option<PathBuf>,
    /// Called once per turn, right after the child spawns and before it is waited on.
    pub on_running: Option<std::sync::Arc<dyn Fn(u32, String) + Send + Sync>>,
    /// Told around every tree kill of a turn (`SpawnRequest::on_kill`).
    pub on_kill: Option<super::subprocess::KillHook>,
    /// (R7, F11-3) The operator brief to make readable inside the throwaway worktree, or `None`. kimi
    /// reads the review in a worktree of HEAD, so a brief that exists only in the caller's working
    /// tree (uncommitted, or committed-but-locally-modified) is absent there; it is copied in at its
    /// repo-relative path before the turn so the prompt's reference resolves to the operator's actual
    /// brief. EVERY turn creates a fresh HEAD worktree, so every turn - primary, read-insist,
    /// continuation and repair - stages it; a read-insist turn in particular re-asks a zero-tool
    /// primary to read the brief, and that worktree must hold the operator's actual brief.
    pub brief: Option<BriefCopy>,
}

/// (R7) A brief to stage into a kimi worktree: its absolute source in the real tree and the
/// repo-relative path the prompt references (resolved in the worktree cwd).
#[derive(Clone)]
pub struct BriefCopy {
    pub src: PathBuf,
    pub rel: String,
}

/// One kimi stream-json line's effect on the tool calls in flight (`Update-ToolFlight`): an
/// `assistant` line carrying `tool_calls` OPENS the first call (key `kimi:<id>`, label
/// `kimi <name> <id>`); a `tool` line CLOSES the call with that `tool_call_id`. A single assistant
/// line may open several calls - only the first is tracked (best-effort; a kimi tool call is a fast
/// read and the stall timer also resets on any stream growth). Suspends the stall timer while a
/// kimi tool call runs.
pub fn kimi_tool_flight(line: &str) -> ToolFlight {
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return ToolFlight::None,
    };
    let role = v.get("role").and_then(|r| r.as_str()).unwrap_or("");
    match role {
        "assistant" => {
            let first = v
                .get("tool_calls")
                .and_then(|t| t.as_array())
                .and_then(|a| a.first());
            if let Some(tc) = first {
                let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("");
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("tool");
                if id.is_empty() {
                    ToolFlight::Open {
                        key: format!("kimi-tool:{name}#"),
                        label: format!("kimi {name}"),
                    }
                } else {
                    ToolFlight::Open {
                        key: format!("kimi:{id}"),
                        label: format!("kimi {name} {id}"),
                    }
                }
            } else {
                ToolFlight::None
            }
        }
        "tool" => {
            let id = v.get("tool_call_id").and_then(|i| i.as_str()).unwrap_or("");
            ToolFlight::Close {
                key: if id.is_empty() {
                    String::new()
                } else {
                    format!("kimi:{id}")
                },
                fallback_prefix: "kimi-tool:".into(),
            }
        }
        _ => ToolFlight::None,
    }
}

/// Run a git subcommand in `repo`, returning trimmed stdout or an error.
fn git_capture(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// (C3) The throwaway worktree a kimi turn runs in (WRITE ISOLATION): kimi `-p` has no
/// tool-restriction flag and WILL execute writes, so the reviewer never runs in the real checkout.
/// A fresh detached worktree of HEAD under the OS temp is created on [`create`], the kimi child runs
/// with its `cwd`, and the worktree is removed when this guard drops (every success / failure /
/// timeout path), with `git worktree prune` after. The orchestrator's strict tree check on the MAIN
/// tree stays as the second guard.
struct KimiWorktree {
    repo: PathBuf,
    path: PathBuf,
    head: String,
    main_dirty: bool,
    removed: bool,
}

/// The directory-name prefix of the throwaway worktree a kimi review runs in (under the system
/// temp directory). `setup::branches` leaves such a worktree out of its "nobody owns this" count.
pub const THROWAWAY_WORKTREE_PREFIX: &str = "c3-kimi-wt-";

impl KimiWorktree {
    fn create(repo: &Path) -> Result<KimiWorktree, String> {
        let head = git_capture(repo, &["rev-parse", "HEAD"])?;
        let main_dirty = !git_capture(repo, &["status", "--porcelain"])?.is_empty();
        let run_id = uuid::Uuid::new_v4().simple().to_string();
        let path = std::env::temp_dir().join(format!("{THROWAWAY_WORKTREE_PREFIX}{run_id}"));
        let path_s = path.to_string_lossy().to_string();
        git_capture(repo, &["worktree", "add", "--detach", &path_s, "HEAD"])?;
        Ok(KimiWorktree {
            repo: repo.to_path_buf(),
            path,
            head,
            main_dirty,
            removed: false,
        })
    }

    /// The handoff `Isolation:` header line (the worktree path, the short HEAD sha, and - when the
    /// main tree was dirty at launch - a note that the reviewer saw the committed HEAD only).
    fn isolation_line(&self) -> String {
        let short: String = self.head.chars().take(12).collect();
        let mut l = format!(
            "Isolation: the reviewer ran in a throwaway git worktree `{}` of HEAD {short} (removed after the run).",
            self.path.display()
        );
        if self.main_dirty {
            l.push_str(
                " The main working tree was dirty at launch; the reviewer saw the committed HEAD only.",
            );
        }
        l
    }

    fn remove(&mut self) {
        if self.removed {
            return;
        }
        self.removed = true;
        let path_s = self.path.to_string_lossy().to_string();
        // The reviewer can run commands in this worktree: a symlink or junction it left there
        // (say a `node_modules` junction into the main checkout) must never be followed by the
        // forced removal below. Unlink every link first, without descending into it.
        unlink_links_inside(&self.path);
        let _ = git_capture(&self.repo, &["worktree", "remove", "--force", &path_s]);
        let _ = git_capture(&self.repo, &["worktree", "prune"]);
        // A belt-and-suspenders directory removal in case `worktree remove` left anything.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Remove every symlink and Windows junction INSIDE `root` without following it: the walk uses
/// `symlink_metadata` (which reports a junction as a link too), removes a link itself
/// (`remove_dir` for a directory link or junction, `remove_file` otherwise) and never descends into
/// one. `root` itself is not touched. Errors are swallowed. Returns how many links were removed.
pub(crate) fn unlink_links_inside(root: &Path) -> usize {
    let mut removed = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            let ft = meta.file_type();
            if ft.is_symlink() {
                if remove_link(&path, &ft).is_ok() {
                    removed += 1;
                }
            } else if ft.is_dir() {
                stack.push(path);
            }
        }
    }
    removed
}

/// Remove one link (never its target).
fn remove_link(path: &Path, ft: &std::fs::FileType) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;
        // A junction carries the directory attribute and is reported as a directory symlink.
        if ft.is_symlink_dir() {
            return std::fs::remove_dir(path);
        }
        std::fs::remove_file(path).or_else(|_| std::fs::remove_dir(path))
    }
    #[cfg(not(windows))]
    {
        let _ = ft;
        std::fs::remove_file(path)
    }
}

impl Drop for KimiWorktree {
    fn drop(&mut self) {
        self.remove();
    }
}

/// (R7 / A1) Whether a brief reference can be staged into the throwaway worktree at the SAME path:
/// a non-empty relative path with no `..` component (it then lies inside the worktree root). An
/// absolute reference resolves against the real tree from anywhere and needs no staging.
pub fn stageable_brief_rel(rel: &str) -> bool {
    let p = Path::new(rel);
    !rel.is_empty()
        && p.is_relative()
        && !p
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// (A1) The preflight of a seat whose engine runs in a throwaway worktree of HEAD (kimi): the
/// prompt tells the reviewer to read the brief at `brief_ref`, relative to its working directory -
/// the worktree. That path resolves there when the brief is staged into the worktree (a stageable
/// relative path whose source can be read) or is committed at HEAD; an absolute reference resolves
/// against the real file. Otherwise the reviewer would answer without its brief (xelixir panel
/// 47cf7308, 2026-11-12: "the required framing brief is missing"), so the seat is refused before
/// anything starts. The refusal text carries " is not usable:" - the wording the panel's re-seat
/// classifies as a preflight-class refusal (`panel::run::is_preflight_refusal`), so a panel
/// replaces the seat from its reserve. `None` = the brief will be readable (or there is none).
pub fn isolated_brief_refusal(
    repo: &Path,
    engine: &str,
    brief_ref: &str,
    brief_path: Option<&Path>,
) -> Option<String> {
    if brief_ref.is_empty() || Path::new(brief_ref).is_absolute() {
        return None;
    }
    let stageable = stageable_brief_rel(brief_ref);
    let readable = match brief_path {
        Some(p) => std::fs::File::open(p)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        None => Err("no brief file was resolved".to_string()),
    };
    if stageable && readable.is_ok() {
        return None;
    }
    let in_head = stageable
        && git_capture(
            repo,
            &[
                "cat-file",
                "-e",
                &format!("HEAD:{}", brief_ref.replace('\\', "/")),
            ],
        )
        .is_ok();
    if in_head {
        return None;
    }
    let why = if !stageable {
        "its path leaves the repository, so it cannot be copied in at the path the prompt names"
            .to_string()
    } else {
        format!(
            "it cannot be read to be copied in ({})",
            readable.err().unwrap_or_default()
        )
    };
    Some(format!(
        "the {engine} seat is not usable: the brief `{brief_ref}` is neither committed at HEAD nor stageable into the reviewer's throwaway worktree of HEAD - {why}; nothing was started (commit the brief, place it inside the repository, or seat a reviewer that reads the live tree)"
    ))
}

/// (A1) How many uncommitted paths the prompt of a worktree-isolated reviewer names.
pub const UNCOMMITTED_PATHS_SHOWN: usize = 20;

/// (A1) The live tree's uncommitted paths at launch (`git status --porcelain=v1 -z`, untracked
/// directories collapsed as git shows them, a rename by its new path), ordinal-sorted. These are
/// what a reviewer in a throwaway worktree of HEAD does not see as the live tree has them. Empty
/// when the tree is clean or git cannot be run.
pub fn uncommitted_paths(repo: &Path) -> Vec<String> {
    let out = match std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["--no-optional-locks", "status", "--porcelain=v1", "-z"])
        .output()
    {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&out);
    let tokens: Vec<&str> = text.split('\0').collect();
    let mut paths: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i];
        i += 1;
        if tok.len() < 4 {
            continue;
        }
        let xy: Vec<char> = tok[0..2].chars().collect();
        // a rename/copy entry is followed by its ORIGINAL path: skip it, keep the new one.
        if xy.iter().any(|c| matches!(c, 'R' | 'C')) && i < tokens.len() {
            i += 1;
        }
        paths.push(tok[3..].to_string());
    }
    paths.sort();
    paths.dedup();
    paths
}

/// (A1) The prompt paragraph that tells a reviewer in a throwaway worktree of HEAD what it cannot
/// see: the live tree's uncommitted paths at launch (at most [`UNCOMMITTED_PATHS_SHOWN`], then the
/// count of the rest), and that the brief was copied in when it is one of them. `""` when the tree
/// was clean (nothing is missing).
pub fn isolation_prompt_line(head_short: &str, uncommitted: &[String], brief_ref: &str) -> String {
    if uncommitted.is_empty() {
        return String::new();
    }
    let shown: Vec<String> = uncommitted
        .iter()
        .take(UNCOMMITTED_PATHS_SHOWN)
        .map(|p| format!("`{p}`"))
        .collect();
    let more = uncommitted.len().saturating_sub(UNCOMMITTED_PATHS_SHOWN);
    let mut l = format!(
        "Isolation: your working directory is a throwaway git worktree of the committed HEAD {head_short}, not the live checkout. These {} path(s) had uncommitted changes in the live checkout at launch, so your worktree holds their committed version or nothing: {}",
        uncommitted.len(),
        shown.join(", ")
    );
    if more > 0 {
        l.push_str(&format!(" (+{more} more)"));
    }
    l.push('.');
    if !brief_ref.is_empty() && stageable_brief_rel(brief_ref) {
        l.push_str(&format!(
            " The brief `{brief_ref}` was copied into your worktree at that path."
        ));
    }
    l
}

/// (A1) [`isolation_prompt_line`] for the live tree at `repo` now (its uncommitted paths and short
/// HEAD). `""` when the tree is clean.
pub fn isolation_prompt(repo: &Path, brief_ref: &str) -> String {
    let paths = uncommitted_paths(repo);
    if paths.is_empty() {
        return String::new();
    }
    let head = git_capture(repo, &["rev-parse", "--short=12", "HEAD"]).unwrap_or_default();
    isolation_prompt_line(&head, &paths, brief_ref)
}

/// The value of the `-p` argument in a built kimi argv (the pointer prompt), for the length guard.
fn argv_prompt_value(args: &[String]) -> Option<&str> {
    args.iter()
        .position(|a| a == "-p")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
}

impl KimiEngine {
    fn inner(&self) -> SubprocessEngine {
        SubprocessEngine::new(EngineKind::Kimi)
    }

    fn files_for(&self, kind: TurnKind) -> &TurnFiles {
        match kind {
            TurnKind::Primary => &self.primary,
            _ => &self.secondary,
        }
    }

    fn run_argv(&self, turn: &TurnRequest) -> Result<Vec<String>, EngineError> {
        match self.inner().plan(&turn.request)? {
            LaunchPlan::Subprocess(a) => Ok(a.args),
            LaunchPlan::Http(_) => Err(EngineError::MissingField("subprocess argv")),
        }
    }

    fn expect_thread(turn: &TurnRequest) -> String {
        match &turn.request.mode {
            Mode::Resume { thread, .. } => thread.clone(),
            _ => String::new(),
        }
    }

    /// Run one turn and return the outcome plus the parsed detail.
    pub fn run_detailed(&self, turn: &TurnRequest) -> Result<KimiRun, EngineError> {
        let args = self.run_argv(turn)?;
        // (B) The argv-length guard: the pointer is short, so this only fires on a pathological
        // composed value - a clear capability refusal before anything launches, never a silent cut.
        if let Some(p) = argv_prompt_value(&args) {
            if p.len() > KIMI_MAX_ARGV_PROMPT {
                return Ok(KimiRun {
                    outcome: AttemptOutcome::ProviderFailure {
                        failure: c3_core::ledger::ProviderFailure {
                            class: "capability".into(),
                            code: String::new(),
                            message: format!(
                                "the kimi argv prompt is {} chars (> {KIMI_MAX_ARGV_PROMPT}); refusing to launch",
                                p.len()
                            ),
                            ..Default::default()
                        },
                        exit_code: None,
                    },
                    turn: KimiTurn::default(),
                    isolation: None,
                });
            }
        }
        // (A) WRITE ISOLATION: run the kimi child in a throwaway worktree of HEAD, never the real
        // checkout. The guard removes it on every return path (Drop). A failure to create it refuses
        // the run (class capability) rather than silently running in the main tree.
        let worktree = match KimiWorktree::create(&self.cwd) {
            Ok(w) => w,
            Err(e) => {
                return Ok(KimiRun {
                    outcome: AttemptOutcome::ProviderFailure {
                        failure: c3_core::ledger::ProviderFailure {
                            class: "capability".into(),
                            code: String::new(),
                            message: format!("kimi isolation worktree could not be created: {e}"),
                            ..Default::default()
                        },
                        exit_code: None,
                    },
                    turn: KimiTurn::default(),
                    isolation: None,
                });
            }
        };
        // (R7) Stage the operator brief into the worktree so the prompt's repo-relative reference
        // resolves to the operator's ACTUAL brief (committed or not) rather than whatever HEAD holds
        // (or nothing). Only a clean in-repo relative path is staged; an absolute / out-of-repo brief
        // reference already resolves from the worktree against the real tree.
        if let Some(brief) = &self.brief {
            let rel = Path::new(&brief.rel);
            if stageable_brief_rel(&brief.rel) {
                let dest = worktree.path.join(rel);
                let staged = dest
                    .parent()
                    .map(std::fs::create_dir_all)
                    .unwrap_or(Ok(()))
                    .and_then(|()| std::fs::copy(&brief.src, &dest).map(|_| ()));
                // (F11-3) The prompt tells the reviewer to read this brief; if it cannot be staged,
                // the review would read stale or missing content. Refuse before launch rather than
                // run a reviewer pointed at the wrong brief.
                if let Err(e) = staged {
                    return Ok(KimiRun {
                        outcome: AttemptOutcome::ProviderFailure {
                            failure: c3_core::ledger::ProviderFailure {
                                class: "capability".into(),
                                code: String::new(),
                                message: format!(
                                    "kimi could not stage the operator brief `{}` into the isolation worktree: {e}",
                                    brief.rel
                                ),
                                ..Default::default()
                            },
                            exit_code: None,
                        },
                        turn: KimiTurn::default(),
                        isolation: None,
                    });
                }
            }
        }
        let isolation = Some(worktree.isolation_line());
        let files = self.files_for(turn.kind);
        let timeout = std::time::Duration::from_secs_f64(turn.request.timeout_sec.max(0.0));
        let is_primary = matches!(turn.kind, TurnKind::Primary);
        let td = |l: &str| kimi_tool_flight(l);
        let spawn = SpawnRequest {
            launcher: &self.launcher,
            argv: &args,
            // (A) the kimi child runs in the throwaway worktree, not the real checkout.
            cwd: &worktree.path,
            prompt_delivery: self.inner().capabilities().prompt_delivery, // PromptFile: empty stdin
            stdin_text: "",
            events_path: &files.events,
            stderr_path: &files.stderr,
            timeout,
            stall_sec: if is_primary { self.stall_sec } else { 0 },
            kick_path: self.kick_path.as_deref(),
            tool_flight: Some(&td),
            on_running: self
                .on_running
                .as_ref()
                .map(|a| a.as_ref() as &dyn Fn(u32, String)),
            on_kill: self.on_kill.as_ref().map(|a| {
                a.as_ref()
                    as &dyn Fn(super::subprocess::KillEvent<'_>) -> super::subprocess::KillRecord
            }),
            // No per-token API-key billing route to guard: the child inherits this process's
            // environment (minus the host markers, scrubbed by `run_turn`).
            env: None,
            // The prompt TEXT travels in argv (`-p`): quote it by the C runtime rules on a batch
            // launcher, like the claude/grok engines (harmless on an `.exe` launcher).
            crt_quote: true,
            budget: None,
        };
        let result = run_turn(&spawn);

        if !result.started {
            return Ok(KimiRun {
                outcome: AttemptOutcome::LaunchFailed {
                    child_exists: false,
                    message: result
                        .error
                        .unwrap_or_else(|| "the kimi process did not start".into()),
                },
                turn: KimiTurn::default(),
                isolation,
            });
        }

        let events_text = read_text(&files.events);
        use super::subprocess::TurnStop;
        let killed = !matches!(result.stop, TurnStop::Exited);
        let allow_partial = killed || result.exit_code != Some(0);
        let events = read_kimi_events(&events_text, allow_partial);

        if killed {
            let conversation = if is_kimi_session(&events.session) {
                ConversationTrust::Candidate(ConversationId(events.session.clone()))
            } else {
                ConversationTrust::None
            };
            let partial = kimi_salvage(&events_text);
            let survivors = result.survivors.clone();
            let kill = result.kill.clone();
            let wall_seconds = result.wall_seconds;
            let outcome = match result.stop {
                TurnStop::Stall => AttemptOutcome::Stopped {
                    kind: c3_core::engine::StopKind::Stall {
                        silent_seconds: result.silent_seconds,
                        tool_open_seconds: result.tool_open_seconds,
                        open_tools: result.open_tools.clone(),
                        last_event: result.last_event.clone(),
                    },
                    partial,
                    survivors,
                    conversation,
                    kill: kill.clone(),
                    wall_seconds,
                },
                TurnStop::Kick => AttemptOutcome::Stopped {
                    kind: c3_core::engine::StopKind::Kick,
                    partial,
                    survivors,
                    conversation,
                    kill: kill.clone(),
                    wall_seconds,
                },
                _ => AttemptOutcome::TimedOut {
                    partial,
                    survivors,
                    conversation,
                    kill: kill.clone(),
                    wall_seconds,
                },
            };
            return Ok(KimiRun {
                outcome,
                turn: KimiTurn::default(),
                isolation,
            });
        }

        let exit_code = result.exit_code.unwrap_or(-1);
        let expect_thread = Self::expect_thread(turn);
        let mut t = kimi_turn_outcome(&events, exit_code, &result.stderr, &expect_thread);
        t.wall_seconds = result.wall_seconds;
        // The zero-tool-call review guard (primary turn, a reviewing purpose): see grok.
        if is_primary && t.ok {
            if let Some((why, class)) = kimi_review_tool_guard(&self.purpose, events.tool_calls) {
                fail(&mut t, &why, &class, &[]);
                // (C3) the orchestrator reads this to drive the read-insist re-ask before failing.
                t.review_guard_fired = true;
            }
        }
        let outcome = to_outcome(&t, files, result.wall_seconds, exit_code);
        Ok(KimiRun {
            outcome,
            turn: t,
            isolation,
        })
    }
}

/// One turn's outcome plus its parsed detail and the `Isolation:` handoff line (the throwaway
/// worktree the turn ran in; `None` only when no worktree was created — a length-guard or
/// worktree-creation refusal).
#[derive(Debug, Clone)]
pub struct KimiRun {
    pub outcome: AttemptOutcome,
    pub turn: KimiTurn,
    pub isolation: Option<String>,
}

fn to_outcome(
    t: &KimiTurn,
    files: &TurnFiles,
    wall_seconds: f64,
    exit_code: i32,
) -> AttemptOutcome {
    if t.ok {
        let conversation = if t.thread.is_empty() {
            ConversationTrust::None
        } else {
            ConversationTrust::Verified(ConversationId(t.thread.clone()))
        };
        return AttemptOutcome::Completed(Reply {
            raw_text: t.reply.clone(),
            structured: parse_structured(&t.reply),
            events_path: files.events.clone(),
            // kimi reports no token usage.
            usage: None,
            wall_seconds,
            conversation,
        });
    }
    let primary = t
        .texts
        .first()
        .cloned()
        .unwrap_or_else(|| t.outcome.clone());
    let (code, message) = convert_from_provider_error_text(&primary);
    let class = if !t.class.is_empty() {
        t.class.clone()
    } else {
        provider_failure_class(&message)
    };
    AttemptOutcome::ProviderFailure {
        failure: c3_core::ledger::ProviderFailure {
            class,
            code,
            message,
            ..Default::default()
        },
        exit_code: Some(exit_code),
    }
}

impl Engine for KimiEngine {
    fn capabilities(&self) -> Capabilities {
        self.inner().capabilities()
    }

    fn plan(&self, request: &Request) -> Result<LaunchPlan, EngineError> {
        self.inner().plan(request)
    }

    fn precheck(&self, _turn: &TurnRequest) -> Result<(), EngineError> {
        // kimi has no launch refusal: the billing route is a managed oauth sign-in (no per-token
        // API key to strip). The sign-in is reported by `providers`.
        Ok(())
    }

    fn run(&self, turn: &TurnRequest) -> Result<AttemptOutcome, EngineError> {
        self.run_detailed(turn).map(|r| r.outcome)
    }

    fn continue_turn(&self, turn: &TurnRequest) -> Result<AttemptOutcome, EngineError> {
        self.run_detailed(turn).map(|r| r.outcome)
    }
}

// --------------------------------------------------------------------------- event parsing

const KIMI_SESSION_RE: &str =
    r"^session_[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$";

/// Whether a string is a kimi session id (`session_<uuid>`).
pub fn is_kimi_session(s: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(KIMI_SESSION_RE).unwrap())
        .is_match(s)
}

fn one_line(s: &str) -> String {
    c3_core::one_line(s)
}

/// The normalized record of one `kimi ... --output-format stream-json` stream.
#[derive(Debug, Clone, Default)]
pub struct KimiEvents {
    pub records: usize,
    /// The number of tool calls (across all `assistant` lines carrying `tool_calls`) - the evidence
    /// the model read the code.
    pub tool_calls: usize,
    /// Why the stream is malformed (`''` when well-formed).
    pub malformed: String,
    /// The reply text: the concatenated `content` of `assistant` lines WITHOUT `tool_calls`
    /// (content that accompanies tool calls is narration and is dropped).
    pub text: String,
    /// (F11-2) The final answer: the `content` of `assistant` lines without `tool_calls` that come
    /// AFTER the last tool call or tool result. Reset whenever a tool call or a tool result arrives,
    /// so text emitted BEFORE a tool call (an interim verdict/placeholder) never stands in as the
    /// reply. With no tool call `final_text == text`.
    pub final_text: String,
    /// The session id of the `session.resume_hint` meta line (`''` when none).
    pub session: String,
    /// A `session.resume_hint` meta line was seen (the end-of-run marker of a clean turn).
    pub has_end: bool,
    /// The message of an in-stream error record (rare; kimi errors come on stderr).
    pub error: String,
}

/// Parse one kimi stream-json stream. `allow_partial_last` lets the last non-empty line be
/// truncated (a killed / non-zero-exit turn).
pub fn read_kimi_events(events_text: &str, allow_partial_last: bool) -> KimiEvents {
    let mut r = KimiEvents::default();
    let lines: Vec<&str> = events_text.split(['\r', '\n']).collect();
    let last_idx = lines.iter().rposition(|l| !l.trim().is_empty());

    for (i, raw) in lines.iter().enumerate() {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        let obj: Value = match serde_json::from_str::<Value>(t) {
            Ok(v) if v.is_object() => v,
            _ => {
                let is_last = Some(i) == last_idx;
                if (!is_last || !allow_partial_last) && r.malformed.is_empty() {
                    r.malformed = format!("line {} is not a JSON object", i + 1);
                }
                continue;
            }
        };
        r.records += 1;
        let role = obj.get("role").and_then(|v| v.as_str()).unwrap_or("");
        match role {
            "assistant" => {
                if let Some(tcs) = obj.get("tool_calls").and_then(|v| v.as_array()) {
                    // A tool-call line: count the calls; its `content` (if any) is narration. A new
                    // read begins, so any answer text so far was interim, not the final reply.
                    r.tool_calls += tcs.len();
                    r.final_text.clear();
                } else if let Some(c) = obj.get("content").and_then(|v| v.as_str()) {
                    r.text.push_str(c);
                    r.final_text.push_str(c);
                }
            }
            // A tool result closes a call; text after it is a fresh answer tail.
            "tool" => {
                r.final_text.clear();
            }
            "meta" => {
                let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if ty == "session.resume_hint" {
                    r.has_end = true;
                    r.session = obj
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                }
            }
            "error" => {
                if r.error.is_empty() {
                    r.error = obj
                        .get("content")
                        .or_else(|| obj.get("message"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                }
            }
            // Any other role is ignored.
            _ => {
                // A defensive catch for a top-level `{"error":"..."}` without a role.
                if role.is_empty() && r.error.is_empty() {
                    if let Some(e) = obj.get("error").and_then(|v| v.as_str()) {
                        r.error = e.to_string();
                    }
                }
            }
        }
    }
    r
}

/// The error line kimi prints on STDERR for a start failure (`error: failed to run prompt: ...`),
/// one-lined; the benign libuv `Assertion failed: ...` shutdown line is skipped. Falls back to the
/// last meaningful stderr line, else `''`.
pub fn kimi_stderr_error(stderr_text: &str) -> String {
    let lines: Vec<&str> = stderr_text
        .split(['\r', '\n'])
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("Assertion failed"))
        .collect();
    if let Some(l) = lines.iter().find(|l| l.starts_with("error:")) {
        return one_line(l.trim_start_matches("error:").trim());
    }
    lines.last().map(|l| one_line(l)).unwrap_or_default()
}

// --------------------------------------------------------------------------- outcome rules

/// The failure rules of one kimi turn.
#[derive(Debug, Clone, Default)]
pub struct KimiTurn {
    pub ok: bool,
    pub outcome: String,
    pub class: String,
    pub texts: Vec<String>,
    pub thread: String,
    pub thread_candidate: String,
    pub reply: String,
    pub structured: bool,
    /// The number of tool calls over the stream (the review guard's input).
    pub tool_calls: usize,
    /// (C3) The zero-tool-call review guard fired on this PRIMARY turn: the reply was otherwise
    /// usable but read no file, so the orchestrator may re-ask (read-insist) before failing.
    pub review_guard_fired: bool,
    pub warnings: Vec<String>,
    /// (A1) The turn's own wall time in seconds (set by [`KimiEngine::run_detailed`]; `0.0` from
    /// [`kimi_turn_outcome`] alone). A guard failure's `ProviderFailure` outcome carries no wall, so
    /// the orchestrator reads this one.
    pub wall_seconds: f64,
}

/// The consultation purposes whose kimi reply must be backed by reading the code: a zero-tool-call
/// answer to one of these cannot have checked anything, so it is not a usable review. (Same set as
/// grok's.)
pub const KIMI_TOOL_REQUIRED_PURPOSES: [&str; 4] =
    ["acceptance", "core-contract", "diff-review", "checkpoint"];

/// The zero-tool-call review guard decision (pure): a turn of one of
/// [`KIMI_TOOL_REQUIRED_PURPOSES`] that made zero tool calls cannot have read the code -> a forced
/// `capability` failure (keeps the reply/thread for the record). `None` otherwise.
pub fn kimi_review_tool_guard(purpose: &str, tool_calls: usize) -> Option<(String, String)> {
    if tool_calls == 0 && KIMI_TOOL_REQUIRED_PURPOSES.contains(&purpose) {
        Some((
            format!(
                "kimi answered without reading any file (0 tool calls) - a {purpose} review must read the code; ask again"
            ),
            "capability".into(),
        ))
    } else {
        None
    }
}

/// Judge one kimi turn. `expect_thread` is the session a resume must land on.
pub fn kimi_turn_outcome(
    events: &KimiEvents,
    exit_code: i32,
    stderr_text: &str,
    expect_thread: &str,
) -> KimiTurn {
    let mut o = KimiTurn {
        tool_calls: events.tool_calls,
        ..Default::default()
    };
    let session = &events.session;
    let is_session = is_kimi_session(session);

    // A non-zero exit is a start/run failure: kimi prints the reason on stderr (bad model id, not
    // signed in, quota). An in-stream error record (rare) is preferred when present.
    if exit_code != 0 {
        let msg = if !events.error.is_empty() {
            one_line(&events.error)
        } else {
            let e = kimi_stderr_error(stderr_text);
            if e.is_empty() {
                format!("kimi exit {exit_code}")
            } else {
                e
            }
        };
        fail(
            &mut o,
            &format!("kimi failed - {msg}"),
            "",
            std::slice::from_ref(&msg),
        );
        if is_session {
            o.thread_candidate = session.clone();
        }
        return o;
    }
    if !events.error.is_empty() {
        fail(
            &mut o,
            &format!("kimi error - {}", one_line(&events.error)),
            "",
            std::slice::from_ref(&events.error),
        );
        return o;
    }
    if !events.malformed.is_empty() {
        fail(
            &mut o,
            &format!("malformed event stream: {}", events.malformed),
            "transport",
            &[],
        );
        if is_session {
            o.thread_candidate = session.clone();
        }
        return o;
    }
    if !events.has_end {
        fail(
            &mut o,
            "no session.resume_hint in the kimi stream (the turn did not finish cleanly)",
            "",
            &[],
        );
        return o;
    }
    if session.is_empty() {
        fail(
            &mut o,
            "the kimi event stream names no session (the resume hint has no session_id)",
            "unknown",
            &[],
        );
        return o;
    }
    if !is_session {
        fail(
            &mut o,
            &format!("the session id '{session}' is not a kimi session id"),
            "unknown",
            &[],
        );
        return o;
    }
    if !expect_thread.is_empty() && session != expect_thread {
        fail(
            &mut o,
            &format!("parent session {expect_thread} not found, kimi used {session}"),
            "unknown",
            &[],
        );
        o.thread_candidate = session.clone();
        return o;
    }
    o.thread = session.clone();
    // (F11-2) A turn that made tool calls but produced NO text after the last one has no final
    // answer: the pre-tool text was an interim schema-shaped progress note / verdict, never the
    // reply. Fail it the way a stream that ended without an answer is failed - do not let the
    // placeholder stand in as the answer. A zero-tool turn keeps `final_text == text` and is owned
    // by the zero-tool review guard. Mirrors the grok adapter.
    // The failure's evidence is the reason alone, NOT the reviewer's interim text: `texts` feed the
    // failure message and its class, and a placeholder that mentions "quota", "timeout" or "auth"
    // would otherwise be recorded as that provider failure (and mark the provider unavailable). The
    // interim text stays in the events file.
    if events.tool_calls > 0 && events.final_text.trim().is_empty() {
        fail(
            &mut o,
            "the kimi turn made tool calls but produced no answer after the last one",
            "",
            &[],
        );
        return o;
    }
    // The reply is the FINAL answer (the text after the last tool call); an interim schema-shaped
    // progress note written before the reading is dropped. With no tool call `final_text == text`.
    o.reply = if events.tool_calls > 0 {
        events.final_text.clone()
    } else {
        events.text.clone()
    };
    if o.reply.trim().is_empty() {
        fail(&mut o, "empty reply", "", &[]);
        return o;
    }
    if o.reply.trim().starts_with('{') {
        o.structured = serde_json::from_str::<Value>(o.reply.trim())
            .map(|v| v.is_object())
            .unwrap_or(false);
    }
    o.ok = true;
    o.outcome = "usable reply".into();
    o
}

fn fail(o: &mut KimiTurn, why: &str, class: &str, texts: &[String]) {
    o.ok = false;
    o.outcome = format!("failed: {why}");
    o.class = class.to_string();
    let mut all: Vec<String> = texts.iter().filter(|t| !t.is_empty()).cloned().collect();
    all.push(why.to_string());
    o.texts = all;
}

// --------------------------------------------------------------------------- salvage

/// Salvage the reply text of a killed turn: the final answer, i.e. the `content` of `assistant`
/// lines without `tool_calls` that come AFTER the last tool call or tool result (an interim
/// schema-shaped progress note written before the reading is never salvaged). With no tool call it
/// is all the text. `None` when that tail is empty - so a turn that read and then died with only
/// pre-tool text salvages nothing, and its placeholder never becomes the reply. Mirrors
/// [`crate::engines::grok::grok_salvage`].
pub fn kimi_salvage(events_text: &str) -> Option<String> {
    let mut all = String::new();
    let mut tail = String::new();
    let mut saw_tool = false;
    for line in events_text.split(['\r', '\n']) {
        let t = line.trim();
        if !t.starts_with('{') {
            continue;
        }
        let obj: Value = match serde_json::from_str(t) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match obj.get("role").and_then(|v| v.as_str()) {
            Some("assistant") => {
                if obj.get("tool_calls").is_some() {
                    // a new read begins: any text so far was an interim note, not the final answer.
                    saw_tool = true;
                    tail.clear();
                } else if let Some(c) = obj.get("content").and_then(|v| v.as_str()) {
                    all.push_str(c);
                    tail.push_str(c);
                }
            }
            // a tool result closes a call: the next answer text is a fresh tail.
            Some("tool") => {
                saw_tool = true;
                tail.clear();
            }
            _ => {}
        }
    }
    let chosen = if saw_tool { tail } else { all };
    if chosen.trim().is_empty() {
        None
    } else {
        Some(chosen)
    }
}

// --------------------------------------------------------------------------- helpers

fn parse_structured(raw_text: &str) -> Option<StructuredReply> {
    let trimmed = raw_text.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    let raw: RawReply = serde_json::from_str(trimmed).ok()?;
    StructuredReply::try_from(raw).ok()
}

fn read_text(path: &Path) -> String {
    std::fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("fixtures/kimi_ok.events.jsonl");
    const TOOLS: &str = include_str!("fixtures/kimi_tools.events.jsonl");
    const RESUMED: &str = include_str!("fixtures/kimi_resumed.events.jsonl");

    #[test]
    fn ok_stream_is_a_usable_reply_with_a_session() {
        let e = read_kimi_events(OK, false);
        assert!(e.malformed.is_empty(), "malformed: {}", e.malformed);
        assert!(e.has_end);
        assert!(is_kimi_session(&e.session), "session: {}", e.session);
        assert_eq!(e.text, "OK");
        assert_eq!(e.tool_calls, 0);
        let t = kimi_turn_outcome(&e, 0, "", "");
        assert!(t.ok, "outcome: {}", t.outcome);
        assert_eq!(t.reply, "OK");
        assert_eq!(t.thread, e.session);
    }

    #[test]
    fn tool_calls_are_counted_and_narration_is_dropped_from_the_reply() {
        let e = read_kimi_events(TOOLS, false);
        assert!(e.malformed.is_empty(), "malformed: {}", e.malformed);
        // Two tool calls in one assistant line, plus a later single one = 3.
        assert_eq!(e.tool_calls, 3, "tool calls across all assistant lines");
        // The reply is the FINAL assistant content only (no "I'll grep..." narration).
        assert!(e.text.starts_with("Found it."), "reply: {:?}", e.text);
        assert!(
            !e.text.contains("I'll grep"),
            "narration leaked: {:?}",
            e.text
        );
        let t = kimi_turn_outcome(&e, 0, "", "");
        assert!(t.ok, "outcome: {}", t.outcome);
        assert_eq!(t.tool_calls, 3);
    }

    #[test]
    fn resume_keeps_the_same_session_id() {
        let e = read_kimi_events(RESUMED, false);
        let sid = e.session.clone();
        assert!(is_kimi_session(&sid));
        // A resume that lands on the expected session is usable.
        let t = kimi_turn_outcome(&e, 0, "", &sid);
        assert!(t.ok, "outcome: {}", t.outcome);
        assert_eq!(t.thread, sid);
        // A resume that lands on a DIFFERENT session fails (lineage/thread mismatch).
        let miss = kimi_turn_outcome(&e, 0, "", "session_00000000-0000-0000-0000-000000000000");
        assert!(!miss.ok);
        assert!(miss.outcome.contains("not found"));
        assert_eq!(miss.thread_candidate, sid);
    }

    #[test]
    fn nonzero_exit_reads_the_error_off_stderr() {
        // A bad-model-id run: empty stdout, the reason on stderr (plus a benign libuv line).
        let stderr = "error: failed to run prompt: config.invalid: Model \"kimi-code/x\" is not configured in config.toml.\nAssertion failed: !(handle->flags & UV_HANDLE_CLOSING), file src\\win\\async.c, line 76";
        let e = read_kimi_events("", true);
        let t = kimi_turn_outcome(&e, 127, stderr, "");
        assert!(!t.ok);
        assert!(
            t.outcome.contains("is not configured"),
            "outcome: {}",
            t.outcome
        );
        assert!(
            !t.outcome.contains("Assertion failed"),
            "libuv noise leaked"
        );
        assert!(t.thread.is_empty());
    }

    #[test]
    fn no_resume_hint_is_a_failure() {
        // A clean exit but no session.resume_hint (the turn did not finish).
        let stream = r#"{"role":"assistant","content":"partial"}"#;
        let e = read_kimi_events(stream, false);
        assert!(!e.has_end);
        let t = kimi_turn_outcome(&e, 0, "", "");
        assert!(!t.ok);
        assert!(t.outcome.contains("no session.resume_hint"));
    }

    #[test]
    fn review_tool_guard_fires_only_on_a_reviewing_purpose_with_zero_tool_calls() {
        let (why, class) = kimi_review_tool_guard("acceptance", 0).expect("acceptance/0 fires");
        assert!(why.contains("0 tool calls"), "message: {why}");
        assert_eq!(class, "capability");
        assert!(kimi_review_tool_guard("acceptance", 3).is_none());
        assert!(kimi_review_tool_guard("framing", 0).is_none());
        for p in KIMI_TOOL_REQUIRED_PURPOSES {
            assert!(kimi_review_tool_guard(p, 0).is_some(), "{p}/0 must fire");
            assert!(
                kimi_review_tool_guard(p, 1).is_none(),
                "{p}/1 must not fire"
            );
        }
        assert!(kimi_review_tool_guard("", 0).is_none());
    }

    #[test]
    fn tool_flight_opens_on_assistant_tool_calls_and_closes_on_tool_result() {
        let open = r#"{"role":"assistant","content":"reading","tool_calls":[{"type":"function","id":"tool_a","function":{"name":"Read","arguments":"{}"}}]}"#;
        let result = r#"{"role":"tool","tool_call_id":"tool_a","content":"1\thi"}"#;
        let answer = r#"{"role":"assistant","content":"done"}"#;
        assert_eq!(kimi_tool_flight(open).delta(), 1);
        assert_eq!(kimi_tool_flight(result).delta(), -1);
        assert_eq!(kimi_tool_flight(answer).delta(), 0);
        assert_eq!(kimi_tool_flight("garbage").delta(), 0);
    }

    #[test]
    fn salvage_keeps_only_final_answer_content() {
        let s = concat!(
            r#"{"role":"assistant","content":"narration","tool_calls":[{"id":"t","function":{"name":"Read"}}]}"#,
            "\n",
            r#"{"role":"tool","tool_call_id":"t","content":"x"}"#,
            "\n",
            r#"{"role":"assistant","content":"the answer"}"#,
        );
        assert_eq!(kimi_salvage(s).as_deref(), Some("the answer"));
        assert!(kimi_salvage("").is_none());
    }

    #[test]
    fn a_pre_tool_verdict_with_no_post_tool_answer_is_rejected() {
        // (F11-2) A schema-valid verdict emitted BEFORE the tool call, then a tool call + result,
        // then a clean resume hint and exit zero with no further assistant content: the turn ran
        // tools but produced no answer after the last one, so it must be rejected (not usable) and
        // salvage must be empty - the pre-tool verdict must never stand in as the reply.
        let stream = concat!(
            r#"{"role":"assistant","content":"{\"verdict\":\"HOLD\",\"reply_markdown\":\"the quota and rate limit code\"}"}"#,
            "\n",
            r#"{"role":"assistant","content":"reading","tool_calls":[{"type":"function","id":"t1","function":{"name":"Read","arguments":"{}"}}]}"#,
            "\n",
            r#"{"role":"tool","tool_call_id":"t1","content":"1\thello"}"#,
            "\n",
            r#"{"role":"meta","type":"session.resume_hint","session_id":"session_c6f82457-e408-4931-b986-a8e2edd596b1","command":"kimi -r"}"#,
        );
        let e = read_kimi_events(stream, false);
        assert!(e.tool_calls > 0, "a tool call was made");
        assert!(
            e.final_text.trim().is_empty(),
            "no answer after the last tool call: {:?}",
            e.final_text
        );
        let t = kimi_turn_outcome(&e, 0, "", "");
        assert!(!t.ok, "the turn is rejected: {}", t.outcome);
        assert!(
            t.outcome.contains("made tool calls but produced no answer"),
            "outcome: {}",
            t.outcome
        );
        // The rejected turn maps to a failure, never a Verified conversation.
        let files = TurnFiles::default();
        // Its evidence is the reason, never the reviewer's interim text: a pre-tool verdict that
        // mentions "quota" must not turn this into a quota failure of the provider.
        assert!(
            !t.texts.iter().any(|x| x.contains("HOLD")),
            "the pre-tool text is not failure evidence: {:?}",
            t.texts
        );
        match to_outcome(&t, &files, 1.0, 0) {
            AttemptOutcome::ProviderFailure { failure, .. } => {
                assert_eq!(failure.class, "unknown", "message: {}", failure.message);
                assert!(
                    failure
                        .message
                        .contains("produced no answer after the last one"),
                    "message: {}",
                    failure.message
                );
            }
            _ => panic!("a rejected turn is a provider failure, not a verified reply"),
        }
        // The pre-tool verdict is not salvaged as the reply.
        assert!(
            kimi_salvage(stream).is_none(),
            "salvage drops the pre-tool verdict: {:?}",
            kimi_salvage(stream)
        );
    }

    #[test]
    fn a_post_tool_answer_is_the_reply_not_the_pre_tool_text() {
        // (F11-2) Pre-tool placeholder, a tool call + result, then the real post-tool answer: the
        // reply (and the salvage) is exactly the post-tool answer.
        let stream = concat!(
            r#"{"role":"assistant","content":"pre-tool placeholder"}"#,
            "\n",
            r#"{"role":"assistant","content":"reading","tool_calls":[{"type":"function","id":"t1","function":{"name":"Read","arguments":"{}"}}]}"#,
            "\n",
            r#"{"role":"tool","tool_call_id":"t1","content":"1\thello"}"#,
            "\n",
            r#"{"role":"assistant","content":"the post-tool answer"}"#,
            "\n",
            r#"{"role":"meta","type":"session.resume_hint","session_id":"session_c6f82457-e408-4931-b986-a8e2edd596b1","command":"kimi -r"}"#,
        );
        let e = read_kimi_events(stream, false);
        assert_eq!(e.final_text, "the post-tool answer");
        let t = kimi_turn_outcome(&e, 0, "", "");
        assert!(t.ok, "outcome: {}", t.outcome);
        assert_eq!(
            t.reply, "the post-tool answer",
            "the reply is only the post-tool answer"
        );
        assert_eq!(
            kimi_salvage(stream).as_deref(),
            Some("the post-tool answer")
        );
    }

    /// A scratch layout: `<base>/wt/sub/` (the worktree) and `<base>/outside/sentinel.txt`.
    fn link_scratch(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!(
            "c3-kimi-links-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let wt = base.join("wt");
        let outside = base.join("outside");
        std::fs::create_dir_all(wt.join("sub")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("sentinel.txt"), "keep me\n").unwrap();
        std::fs::write(wt.join("sub").join("own.txt"), "worktree file\n").unwrap();
        (base, wt, outside)
    }

    fn assert_link_gone_and_sentinel_kept(base: &Path, wt: &Path, outside: &Path, link: &Path) {
        assert!(
            std::fs::symlink_metadata(link).is_err(),
            "the link inside the worktree is still there"
        );
        assert!(
            outside.join("sentinel.txt").exists(),
            "the link's target was emptied"
        );
        // The worktree's own content is untouched by the unlink pass; the removal after it is safe.
        assert!(wt.join("sub").join("own.txt").exists());
        std::fs::remove_dir_all(wt).unwrap();
        assert!(outside.join("sentinel.txt").exists());
        let _ = std::fs::remove_dir_all(base);
    }

    // Windows only: a junction is made with `mklink /J`, which exists only on Windows (and needs no
    // privilege, unlike a symlink there). The symlink variant below covers the other platforms.
    #[cfg(windows)]
    #[test]
    fn unlink_removes_a_junction_without_following_it() {
        let (base, wt, outside) = link_scratch("junction");
        let link = wt.join("sub").join("node_modules");
        let ok = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "mklink /J failed");
        assert!(
            link.join("sentinel.txt").exists(),
            "the junction does not resolve"
        );
        assert_eq!(unlink_links_inside(&wt), 1);
        assert_link_gone_and_sentinel_kept(&base, &wt, &outside, &link);
    }

    #[cfg(not(windows))]
    #[test]
    fn unlink_removes_a_symlink_without_following_it() {
        let (base, wt, outside) = link_scratch("symlink");
        let link = wt.join("sub").join("node_modules");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        assert!(
            link.join("sentinel.txt").exists(),
            "the symlink does not resolve"
        );
        assert_eq!(unlink_links_inside(&wt), 1);
        assert_link_gone_and_sentinel_kept(&base, &wt, &outside, &link);
    }

    /// A scratch git repo with one commit holding `committed.md` (identity set repo-locally).
    fn brief_repo(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "c3-kimi-brief-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let repo = base.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "t@e.com"],
            vec!["config", "user.name", "T"],
        ] {
            git_capture(&repo, &args).unwrap();
        }
        std::fs::write(repo.join("committed.md"), "# c\n").unwrap();
        git_capture(&repo, &["add", "-A"]).unwrap();
        git_capture(&repo, &["commit", "-q", "-m", "init"]).unwrap();
        repo
    }

    // (A1) the preflight of a worktree-isolated seat: refused only when the brief can reach the
    // worktree neither by staging nor through HEAD; the refusal is preflight-class (re-seated).
    #[test]
    fn the_isolated_brief_preflight_refuses_only_an_unreachable_brief() {
        assert!(stageable_brief_rel(".collab/t/handoffs/01.md"));
        assert!(!stageable_brief_rel("../notes/brief.md"));
        assert!(!stageable_brief_rel(""));
        let repo = brief_repo("pre");
        // no brief, an absolute brief: nothing to refuse.
        assert!(isolated_brief_refusal(&repo, "kimi", "", None).is_none());
        let abs = repo.join("committed.md");
        let abs_s = abs.to_string_lossy().to_string();
        assert!(isolated_brief_refusal(&repo, "kimi", &abs_s, Some(&abs)).is_none());
        // an UNTRACKED brief inside the repository is staged: not refused.
        std::fs::create_dir_all(repo.join(".collab/t/handoffs")).unwrap();
        let untracked = repo.join(".collab/t/handoffs/01-claude-framing.md");
        std::fs::write(&untracked, "# brief\n").unwrap();
        assert!(isolated_brief_refusal(
            &repo,
            "kimi",
            ".collab/t/handoffs/01-claude-framing.md",
            Some(&untracked)
        )
        .is_none());
        // a committed brief is in HEAD even when its source cannot be opened.
        assert!(isolated_brief_refusal(&repo, "kimi", "committed.md", None).is_none());
        // a brief outside the repository named by a relative path: neither in HEAD nor stageable.
        let outside = repo.parent().unwrap().join("notes");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("brief.md"), "# b\n").unwrap();
        let r = isolated_brief_refusal(
            &repo,
            "kimi",
            "../notes/brief.md",
            Some(&outside.join("brief.md")),
        )
        .expect("an unreachable brief is refused");
        assert!(r.starts_with("the kimi seat is not usable: the brief `../notes/brief.md`"));
        assert!(r.contains("nothing was started"), "{r}");
        assert!(crate::panel::run::is_preflight_refusal(&r), "{r}");
        // an untracked in-repo brief that cannot be resolved (no source) and is not in HEAD.
        let r2 = isolated_brief_refusal(&repo, "kimi", "missing.md", None).unwrap();
        assert!(r2.contains("cannot be read"), "{r2}");
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }

    // (A1) the reviewer is told the uncommitted paths: at most 20 named, then the count.
    #[test]
    fn the_isolation_prompt_names_the_uncommitted_paths() {
        assert_eq!(isolation_prompt_line("abc", &[], "b.md"), "");
        let paths: Vec<String> = (0..25).map(|i| format!("src/f{i:02}.rs")).collect();
        let l = isolation_prompt_line("abc123", &paths, ".collab/t/h/01.md");
        assert!(l.starts_with("Isolation: your working directory is a throwaway git worktree of the committed HEAD abc123"), "{l}");
        assert!(
            l.contains("These 25 path(s) had uncommitted changes"),
            "{l}"
        );
        assert!(
            l.contains("`src/f19.rs`") && !l.contains("`src/f20.rs`"),
            "{l}"
        );
        assert!(l.contains("(+5 more)."), "{l}");
        assert!(l.ends_with(
            "The brief `.collab/t/h/01.md` was copied into your worktree at that path."
        ));
        // from a live repo: an untracked file and a modified one are listed.
        let repo = brief_repo("iso");
        assert_eq!(isolation_prompt(&repo, ""), "");
        std::fs::write(repo.join("committed.md"), "# changed\n").unwrap();
        std::fs::write(repo.join("new.txt"), "n\n").unwrap();
        assert_eq!(
            uncommitted_paths(&repo),
            vec!["committed.md".to_string(), "new.txt".to_string()]
        );
        let live = isolation_prompt(&repo, "new.txt");
        assert!(live.contains("`committed.md`, `new.txt`."), "{live}");
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }
}
