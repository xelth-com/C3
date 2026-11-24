//! (release-0.2.5 wave B1) The revision a reviewer reads.
//!
//! * Which run gets a throwaway worktree ([`plan_isolation`]): `c3 consult --rev <rev>` for every
//!   engine (codex, agy, muse, claude, grok, kimi run in it; (R5, F19-1) the http engine builds its
//!   pack from it); the kimi engine always (a worktree of HEAD: it has no tool-restriction flag); a
//!   claude-engine reviewer on a `diff-review` / `acceptance` purpose (a frozen worktree of HEAD, so
//!   a parallel session editing the tree cannot change what it reads).
//! * Making it and staging the brief, the task's `handoffs/` and `state.md` into it at their
//!   repo-relative paths ([`setup_review_tree`]; nothing is ever read through a link), and - after
//!   the last turn - the reviewer's own writes in it ([`ReviewTree::changes`], git-ignored paths
//!   included): that is what the read-only check watches, while a change of the live tree during
//!   the run is at most a warning.
//! * `--diff <range>` ([`plan_diff`], [`write_patch`]): `git diff <range>` (ignored and
//!   `.c3ignore`d files left out, secret-shaped values redacted) written into the task's handoffs
//!   as `<stem>.<sha8>.patch` - one file per content - next to the brief and named in the prompt.
//! * The warnings: a brief that names a git range given to an engine without a shell and without
//!   `--diff` ([`named_ranges`]); `--rev` resuming a thread recorded on another revision
//!   ([`resume_revision_warning`]).
//! * (0.2.5 D7) A resume of a worktree-isolated engine's thread makes its worktree at the parent
//!   run's recorded path ([`plan_resume_worktree`]; the ledger's `worktree.path`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::consult::revision::{self, RevisionInfo};
use crate::engines::worktree::{self, ThrowawayWorktree};

/// The purposes on which a claude-engine reviewer reads a frozen worktree of HEAD by default.
pub const FROZEN_HEAD_PURPOSES: [&str; 2] = ["diff-review", "acceptance"];

/// Why a run reads a throwaway worktree (the ledger's `worktree.reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// `--rev <rev>`.
    Rev,
    /// The engine never runs in the live checkout (kimi).
    Engine,
    /// A claude-engine diff-review / acceptance: a frozen tree of HEAD.
    FrozenHead,
}

impl Reason {
    pub fn token(self) -> &'static str {
        match self {
            Reason::Rev => "rev",
            Reason::Engine => "engine",
            Reason::FrozenHead => "frozen-head",
        }
    }
}

/// The throwaway worktree a run will read (decided before anything starts; made in `run_live`).
#[derive(Debug, Clone)]
pub struct IsolationPlan {
    pub reason: Reason,
    /// The revision as given (`--rev`), or `HEAD`.
    pub rev: String,
    /// The full sha at plan time; `""` when HEAD does not resolve (a repository without commits -
    /// making the worktree then fails the run as a capability failure, as it always did for kimi).
    pub sha: String,
    /// (0.2.5 D7) Where the worktree is made when this run RESUMES a thread of a worktree-isolated
    /// engine: the parent run's worktree path ([`plan_resume_worktree`]); `None` = a fresh random
    /// path.
    pub at: Option<PathBuf>,
    /// (0.2.5 D7) The sentence a kimi "created under a different directory" failure adds when this
    /// run resumes a thread whose worktree path is not known or could not be reused (`""`: none).
    pub resume_note: String,
}

impl IsolationPlan {
    pub fn is_rev(&self) -> bool {
        self.reason == Reason::Rev
    }

    /// What `git worktree add` checks out: the planned sha, else `HEAD`.
    pub fn checkout(&self) -> &str {
        if self.sha.is_empty() {
            "HEAD"
        } else {
            &self.sha
        }
    }

    /// The revision as a refusal names it: `HEAD`, or `revision <sha12>`.
    pub fn label(&self) -> String {
        if self.is_rev() {
            format!("revision {}", short(&self.sha))
        } else {
            "HEAD".to_string()
        }
    }
}

fn short(sha: &str) -> String {
    sha.chars().take(12).collect()
}

/// The longest revision token `--rev` / `--diff` take (characters).
pub const REV_TOKEN_MAX: usize = 256;

/// A `--rev` value is one revision token: not empty, no blanks or control characters (a line
/// break included), not starting with `-` (git would read it as an option), no `..` (that is a
/// range), at most [`REV_TOKEN_MAX`] characters.
pub fn rev_token_ok(rev: &str) -> bool {
    let r = rev.trim();
    !r.is_empty()
        && r.chars().count() <= REV_TOKEN_MAX
        && !r.starts_with('-')
        && !r.chars().any(|c| c.is_whitespace() || c.is_control())
        && !r.contains("..")
}

/// (R5, F19-5) The one syntax check of a `--rev` value, shared by the CLI (`args::validate`) and the
/// MCP tool (before it starts anything): `Err` is the refusal text.
pub fn check_rev_value(rev: &str) -> Result<(), String> {
    if rev_token_ok(rev) {
        Ok(())
    } else {
        Err(format!(
            "-Rev '{}' must be one revision (a commit, a branch or a tag; no spaces or control characters, not starting with '-', at most {REV_TOKEN_MAX} characters).",
            rev.escape_debug()
        ))
    }
}

/// `--rev` resolved to the full sha of its commit, or the refusal (nothing was started).
pub fn resolve_rev(repo: &Path, rev: &str) -> Result<String, String> {
    worktree::resolve_commit(repo, rev).map_err(|why| format!("-Rev {why}; nothing was started."))
}

/// Which throwaway worktree (if any) a run of `engine` on `purpose` reads: `--rev` (resolved; a
/// revision git does not know is refused), else the engine's own isolation (`spec_isolated`: kimi),
/// else a claude-engine diff-review / acceptance (frozen HEAD), else none. (R5, F19-1) The http
/// engine gets one under `--rev` only: its pack is built from that worktree, never from the live
/// tree; without `--rev` its pack is of the live tree as before.
pub fn plan_isolation(
    repo: &Path,
    engine: &str,
    purpose: &str,
    rev: &str,
    spec_isolated: bool,
) -> Result<Option<IsolationPlan>, String> {
    let head = || worktree::resolve_commit(repo, "HEAD").unwrap_or_default();
    if !rev.trim().is_empty() {
        return Ok(Some(IsolationPlan {
            reason: Reason::Rev,
            rev: rev.trim().to_string(),
            sha: resolve_rev(repo, rev)?,
            at: None,
            resume_note: String::new(),
        }));
    }
    if engine == "http" {
        return Ok(None);
    }
    if spec_isolated {
        return Ok(Some(IsolationPlan {
            reason: Reason::Engine,
            rev: "HEAD".into(),
            sha: head(),
            at: None,
            resume_note: String::new(),
        }));
    }
    if engine == "claude" && FROZEN_HEAD_PURPOSES.contains(&purpose) {
        return Ok(Some(IsolationPlan {
            reason: Reason::FrozenHead,
            rev: "HEAD".into(),
            sha: head(),
            at: None,
            resume_note: String::new(),
        }));
    }
    Ok(None)
}

/// Whether the engine's reviewer may run commands (and so `git diff` a range a brief names):
/// codex only - agy, muse, claude and grok have no shell in a consultation, kimi is told not to
/// run any, and the http reviewer receives only a pack.
pub fn engine_runs_commands(engine: &str) -> bool {
    engine == "codex"
}

/// Whether one side of a `a..b` match looks like a git revision rather than prose or code (`0..n`,
/// `start..end`): `HEAD...`, a name with `/ ~ ^ @`, an abbreviated or full sha, a usual branch name
/// or a version tag.
fn revision_like(t: &str) -> bool {
    let hex = t.len() >= 7 && t.len() <= 40 && t.bytes().all(|b| b.is_ascii_hexdigit());
    let tag = {
        let rest = t.strip_prefix('v').unwrap_or("");
        !rest.is_empty()
            && rest
                .split('.')
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    };
    t.starts_with("HEAD")
        || t.contains(['/', '~', '^', '@'])
        || hex
        || tag
        || ["main", "master", "develop", "trunk", "origin"].contains(&t)
}

/// The git ranges (`a..b`, `a...b`) a brief or an ask names, in first-seen order: both sides
/// revision tokens, neither all digits, and at least one side [`revision_like`] - so `0..n` or
/// `start..end` in quoted code is not taken for one.
pub fn named_ranges(text: &str) -> Vec<String> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(
            r"(?:^|[^A-Za-z0-9_./~^@{}-])([A-Za-z0-9_][A-Za-z0-9_./~^@{}-]*?)(\.\.\.?)([A-Za-z0-9_][A-Za-z0-9_./~^@{}-]*)",
        )
        .unwrap()
    });
    let mut out: Vec<String> = Vec::new();
    for c in re.captures_iter(text) {
        let a = c[1].trim_end_matches('.');
        let b = c[3].trim_end_matches(['.', ',', ';', ':', ')']);
        if a.is_empty() || b.is_empty() {
            continue;
        }
        let digits = |t: &str| t.bytes().all(|x| x.is_ascii_digit());
        if digits(a) || digits(b) || !(revision_like(a) || revision_like(b)) {
            continue;
        }
        let r = format!("{a}{}{b}", &c[2]);
        if !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

/// The warning for a brief (or ask) that names a git range given to an engine without a shell and
/// without `--diff`: the reviewer cannot run `git diff` (grok spent ~15 tool calls on `.git`
/// archaeology in such a run). `None` when nothing applies.
pub fn range_without_shell_warning(engine: &str, diff: &str, texts: &[&str]) -> Option<String> {
    if engine_runs_commands(engine) || !diff.trim().is_empty() {
        return None;
    }
    let mut ranges: Vec<String> = Vec::new();
    for t in texts {
        for r in named_ranges(t) {
            if !ranges.contains(&r) {
                ranges.push(r);
            }
        }
    }
    let first = ranges.first()?;
    Some(format!(
        "the brief names the git range `{first}`, but the {engine} reviewer has no shell to run git: pass -Diff {first} so the change reaches it as a patch file"
    ))
}

/// (`--rev` with `--mode resume|fork`) The warning when the continued thread was recorded on another
/// revision than this run reviews: the reviewer's earlier context describes other code. The parent
/// entry's revision is its `worktree.sha` (a run in a throwaway worktree), else its `base_commit`.
pub fn resume_revision_warning(
    mode: &str,
    parent_thread: &str,
    parent: Option<&c3_core::ledger::LedgerEntry>,
    sha: &str,
    rev: &str,
) -> Option<String> {
    if parent_thread.is_empty() || sha.is_empty() || !matches!(mode, "resume" | "fork") {
        return None;
    }
    let e = parent?;
    let recorded = e
        .extra
        .get("worktree")
        .and_then(|w| w.get("sha"))
        .and_then(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| e.base_commit.clone());
    if recorded.is_empty()
        || recorded == "unknown"
        || recorded == "none"
        || recorded.starts_with(sha)
        || sha.starts_with(&recorded)
    {
        return None;
    }
    Some(format!(
        "-Rev {rev}: the {mode} thread {parent_thread} was recorded on revision {} (consult n={}), this run reviews {}; the reviewer's earlier context describes another revision",
        short(&recorded),
        e.n,
        short(sha)
    ))
}

// ------------------------------------------------------------------------------- the worktree

/// The run's throwaway worktree, with the snapshot taken right after the staging: what the
/// reviewer's own writes are measured against.
pub struct ReviewTree {
    wt: ThrowawayWorktree,
    /// The worktree's copy of the collab directory (when the collab directory lies in the
    /// repository).
    collab: Option<PathBuf>,
    /// The collab directory's repo-relative path with a trailing `/` (for the changed names).
    collab_shown: String,
    /// (R5, F20-4) The snapshot the reviewer's writes are measured against: tracked, untracked AND
    /// git-ignored files of the checkout.
    before: RevisionInfo,
    before_collab: HashMap<String, String>,
    /// (R5, F18-1) The worktree's fingerprint right after the staging, as the run's records name
    /// it (the usual one: git-ignored files excluded, like the live tree's).
    reviewed: RevisionInfo,
    /// (R5, F18-7) Whether a `handoffs/` directory was actually staged (at least one file copied).
    handoffs_staged: bool,
    /// (R5, F18-5) How many collab entries were NOT staged because they are links (a linked
    /// `handoffs/` or `state.md`, or a link below `handoffs/`).
    links_skipped: usize,
    /// (0.2.5 R8, F51-2) The brief as it was STAGED: the sha256 of the bytes written into the
    /// worktree and the full sha of the commit they were materialised from (`None`: the live file).
    staged_brief: Option<(String, Option<String>)>,
}

/// What [`setup_review_tree`] stages.
pub struct StageInputs<'a> {
    pub repo_root: &'a Path,
    pub collab_root: &'a Path,
    pub task: &'a str,
    pub brief_ref: &'a str,
    pub brief_path: Option<&'a Path>,
    /// The engine, as a staging refusal names the seat.
    pub engine: &'a str,
}

/// Why [`setup_review_tree`] could not make the run's throwaway worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupError {
    /// The worktree itself could not be made: the run's capability failure (nothing was started).
    Capability(String),
    /// (F11-3, F11-4) Something could not be STAGED into it (an unsafe destination - a link the
    /// checked-out revision carries -, a brief that cannot be read nor materialised from HEAD, a
    /// write error): a typed pre-start refusal with preflight-class wording
    /// ([`worktree::staging_refusal`]) - no reviewer starts, and a panel re-seats an optional seat
    /// from its reserve exactly as for a preflight refusal.
    Refused(String),
}

/// Whether a handoff file is left out of the staging: the engines' event streams (large, never
/// review material) - (R5, F18-7) by every name the bridge writes them under: `*.events.jsonl`
/// and a bare `events.jsonl`.
fn skip_staging(p: &Path) -> bool {
    p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
        let n = n.to_ascii_lowercase();
        n == "events.jsonl" || n.ends_with(".events.jsonl")
    })
}

/// (R5, F18-5 / 0.2.5 D4) How a staging source that could not be reached through the repository's
/// handles counts: a link on the way (any component, the entry itself) is skipped and counted, an
/// entry that is not there or not of the asked kind is simply not staged, anything else refuses the
/// run (`Err` with the words).
fn source_step(e: crate::contain::StepError, shown: &Path) -> Result<bool, String> {
    use crate::contain::Step;
    match e.why {
        Step::Link { .. } => Ok(true),
        Step::Missing | Step::NotDir | Step::NotFile | Step::BadPath => Ok(false),
        why => Err(format!("{} {why}", shown.display())),
    }
}

/// Make the run's throwaway worktree of `plan` and stage into it, at their repo-relative paths: the
/// task's `handoffs/` (event streams left out) and `state.md` when the collab directory lies in the
/// repository, then the brief (a [`worktree::stageable_rel`] reference) - so an uncommitted brief
/// and the earlier replies are readable there. (F11-4) The brief's bytes are the live file's, else
/// the regular file committed at HEAD (or at the worktree's revision) - [`worktree::brief_bytes`];
/// a stageable brief is ALWAYS staged or the run is refused, so the prompt's "copied into your
/// worktree" sentence is only ever sent when the copy was made. (F11-3) Nothing is ever written
/// through a link: the brief's destination is checked in the revision's tree
/// ([`worktree::rev_destination_problem`]) and on disk ([`ThrowawayWorktree::stage_bytes`]).
/// (R5, F18-5) Nothing is ever READ through a link either: a brief that is a link (or reached
/// through one from outside the repository) refuses the run; a `handoffs/` directory or `state.md`
/// that is one, and every link below `handoffs/`, is skipped and counted
/// ([`ReviewTree::links_skipped`]) - nothing outside the repository is copied into a worktree.
/// `Err(Capability)` when the worktree cannot be made, `Err(Refused)` when staging cannot be done
/// safely ([`SetupError`]); either way nothing was started.
///
/// (0.2.5 D4, F29-2 / F18-5) The sources are reached through HANDLES: the repository is opened once
/// ([`crate::contain::Root::open_anchor`]), `handoffs/` and `state.md` are opened from it one
/// component at a time without following any (a link anywhere on the way - one that stays inside
/// the repository included - is skipped and counted), and the files are read through handles
/// opened without following them; the brief likewise ([`worktree::brief_bytes`]). Nothing checked
/// first is opened again by its name afterwards.
///
/// (0.2.5 D7) A run that resumes a thread of a worktree-isolated engine makes the worktree at the
/// parent run's path ([`IsolationPlan::at`]); a path that is taken refuses the seat before anything
/// starts ([`worktree::occupied_refusal`]) - nothing there is removed or reused.
pub fn setup_review_tree(plan: &IsolationPlan, s: &StageInputs) -> Result<ReviewTree, SetupError> {
    let wt = ThrowawayWorktree::create_at(s.repo_root, plan.checkout(), plan.at.as_deref())
        .map_err(|e| match e {
            worktree::CreateError::Occupied(p) => {
                SetupError::Refused(worktree::occupied_refusal(s.engine, &p))
            }
            worktree::CreateError::Failed(e) => SetupError::Capability(format!(
                "the reviewer's throwaway worktree could not be created: {e}"
            )),
        })?;
    let label = plan.label();
    let refused = |what: &str, why: String| {
        SetupError::Refused(worktree::staging_refusal(s.engine, what, &label, &why))
    };
    let collab_rel = c3_core::paths::repo_relative(s.repo_root, s.collab_root)
        .filter(|r| !r.is_empty() && worktree::stageable_rel(r));
    let mut collab: Option<PathBuf> = None;
    let mut handoffs_staged = false;
    let mut links_skipped = 0usize;
    let mut staged_brief: Option<(String, Option<String>)> = None;
    if let Some(cr) = &collab_rel {
        let task_dir = s.collab_root.join(s.task);
        let handoffs = task_dir.join("handoffs");
        let state = task_dir.join("state.md");
        let repo = crate::contain::Root::open_anchor(s.repo_root).map_err(|e| {
            refused(
                "the task's handoffs",
                format!("the repository {} {}", s.repo_root.display(), e.words()),
            )
        })?;
        let task_rel = Path::new(cr).join(s.task);
        match repo.open_dir(&task_rel.join("handoffs"), false) {
            Ok(src) => {
                let st = wt
                    .stage_dir(
                        &src,
                        &handoffs,
                        &format!("{cr}/{}/handoffs", s.task),
                        &skip_staging,
                    )
                    .map_err(|e| refused("the task's handoffs", e))?;
                links_skipped += st.links_skipped;
                handoffs_staged = st.files > 0;
            }
            Err(e) => {
                if source_step(e, &handoffs).map_err(|e| refused("the task's handoffs", e))? {
                    links_skipped += 1;
                }
            }
        }
        match repo.read(&task_rel.join("state.md")) {
            Ok(bytes) => wt
                .stage_bytes(&bytes, &format!("{cr}/{}/state.md", s.task))
                .map_err(|e| refused("the task's state.md", e))?,
            Err(e) => {
                if source_step(e, &state).map_err(|e| refused("the task's state.md", e))? {
                    links_skipped += 1;
                }
            }
        }
        collab = Some(wt.path().join(cr));
    }
    if !s.brief_ref.is_empty() && worktree::stageable_rel(s.brief_ref) {
        // (F11-3) the prompt tells the reviewer to read this brief; if it cannot be staged the
        // review would read stale or missing content - refuse before launch instead.
        let what = format!("the brief `{}`", s.brief_ref);
        if let Some(p) = worktree::rev_destination_problem(s.repo_root, wt.sha(), s.brief_ref) {
            return Err(refused(&what, p));
        }
        let src = worktree::brief_bytes(s.repo_root, s.brief_ref, s.brief_path, wt.sha())
            .map_err(|e| refused(&what, e))?;
        wt.stage_bytes(&src.bytes, s.brief_ref)
            .map_err(|e| refused(&what, e))?;
        staged_brief = Some((c3_core::sha256_hex(&src.bytes), src.from_revision));
    }
    let before = revision::revision_info_with_ignored(wt.path(), collab.as_deref());
    let reviewed = revision::revision_info(wt.path(), collab.as_deref());
    let before_collab = collab
        .as_deref()
        .map(crate::engines::tree_check::collab_snapshot)
        .unwrap_or_default();
    Ok(ReviewTree {
        wt,
        collab,
        collab_shown: collab_rel.map(|r| format!("{r}/")).unwrap_or_default(),
        before,
        before_collab,
        reviewed,
        handoffs_staged,
        links_skipped,
        staged_brief,
    })
}

impl ReviewTree {
    /// The worktree's directory (every turn's working directory).
    pub fn path(&self) -> &Path {
        self.wt.path()
    }

    /// The full sha the worktree checks out.
    pub fn sha(&self) -> &str {
        self.wt.sha()
    }

    /// (R5, F18-1) The worktree's fingerprint right after the staging: what a `--rev` run's records
    /// name as reviewed (base commit = the worktree's sha).
    pub fn reviewed_info(&self) -> &RevisionInfo {
        &self.reviewed
    }

    /// (R5, F18-7) Whether a `handoffs/` directory was staged (at least one file copied).
    pub fn handoffs_staged(&self) -> bool {
        self.handoffs_staged
    }

    /// (R5, F18-5) How many collab entries were skipped because they are links.
    pub fn links_skipped(&self) -> usize {
        self.links_skipped
    }

    /// (0.2.5 R8, F51-2) The brief as it was staged: the sha256 of the bytes written and the full
    /// sha of the commit they came from (`None`: the live file); `None` when no brief was staged.
    pub fn staged_brief(&self) -> Option<&(String, Option<String>)> {
        self.staged_brief.as_ref()
    }

    /// (R5, F18-6) The links removed right after the checkout because they led outside the
    /// worktree ([`ThrowawayWorktree::outside_links`]).
    pub fn outside_links(&self) -> &[String] {
        self.wt.outside_links()
    }

    /// The handoff `Isolation:` line ([`ThrowawayWorktree::isolation_line`]).
    pub fn isolation_line(&self, plan: &IsolationPlan) -> String {
        self.wt
            .isolation_line(plan.is_rev().then_some(plan.rev.as_str()))
    }

    /// The worktree-relative paths whose content appeared, disappeared or changed since the
    /// staging, ordinal-sorted: (R5, F20-4) every file of the checkout - tracked, untracked AND
    /// git-ignored (`git status --ignored`) - and every file of the staged collab copy. A path
    /// git does not see as a file (an empty directory) is not compared. Only the reviewer's
    /// process tree runs there, so any entry is the reviewer's own write.
    pub fn changes(&self) -> Vec<String> {
        let after = revision::revision_info_with_ignored(self.wt.path(), self.collab.as_deref());
        let mut paths = revision::compare_tree_content(&self.before, &after).paths;
        if let Some(c) = &self.collab {
            let now = crate::engines::tree_check::collab_snapshot(c);
            let mut keys: std::collections::BTreeSet<&String> = std::collections::BTreeSet::new();
            keys.extend(self.before_collab.keys());
            keys.extend(now.keys());
            for k in keys {
                if self.before_collab.get(k) != now.get(k) {
                    paths.push(format!("{}{k}", self.collab_shown));
                }
            }
        }
        paths.sort();
        paths.dedup();
        paths
    }
}

/// The ledger's `worktree` record of a run that read a throwaway worktree: `{reason, rev, sha}`,
/// and (0.2.5 D7) `path` - the worktree's directory, when it was made: a run that resumes this
/// run's thread makes its own worktree there ([`plan_resume_worktree`]).
pub fn ledger_worktree_value(
    plan: &IsolationPlan,
    sha: &str,
    path: Option<&Path>,
) -> serde_json::Value {
    let mut v = serde_json::json!({
        "reason": plan.reason.token(),
        "rev": plan.rev,
        "sha": if sha.is_empty() { plan.sha.as_str() } else { sha },
    });
    if let Some(p) = path {
        v["path"] = serde_json::Value::String(p.to_string_lossy().to_string());
    }
    v
}

/// (0.2.5 D7) The worktree path a run that RESUMES `parent_thread` of a worktree-isolated engine
/// (kimi, agy) makes its throwaway worktree at: the parent run's (its ledger entry's
/// `worktree.path`), because such an engine binds its session to the directory it was made in
/// (kimi: `Session "<id>" was created under a different directory.`). Sets [`IsolationPlan::at`]
/// when the recorded path has C3's shape ([`worktree::reusable_worktree_path`]; whether it is free
/// is decided when the worktree is made). A path of another shape is not used: the run gets a fresh
/// path and the returned run warning says the parent's path could not be reused. A parent without
/// a recorded path (an older build, or a parent that did not run isolated) gets a fresh path as
/// before, without a warning. In both fallbacks [`IsolationPlan::resume_note`] is the sentence a
/// kimi "different directory" failure adds: start a new thread.
pub fn plan_resume_worktree(
    plan: &mut IsolationPlan,
    parent_thread: &str,
    parent: Option<&c3_core::ledger::LedgerEntry>,
) -> Option<String> {
    let recorded = parent
        .and_then(|e| e.extra.get("worktree"))
        .and_then(|w| w.get("path"))
        .and_then(|p| p.as_str())
        .filter(|p| !p.is_empty());
    let Some(recorded) = recorded else {
        plan.resume_note = format!(
            "The thread {parent_thread} was started where its directory is not known (its run recorded no worktree path), so it cannot be resumed: start a new thread (--mode new)."
        );
        return None;
    };
    match worktree::reusable_worktree_path(recorded) {
        Ok(p) => {
            plan.at = Some(p);
            None
        }
        Err(why) => {
            plan.resume_note = format!(
                "The thread {parent_thread} was started in {recorded}, which could not be reused, so it cannot be resumed: start a new thread (--mode new)."
            );
            Some(format!(
                "the parent's worktree path {recorded} could not be reused ({why}): the resumed thread {parent_thread} runs in a fresh throwaway worktree, where the engine may not find its session"
            ))
        }
    }
}

// ------------------------------------------------------------------------------- --diff

/// A parsed `--diff` value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffSpec {
    /// `from..to` / `from...to`.
    Range {
        from: String,
        sep: &'static str,
        to: String,
    },
    /// One revision: diffed against the working tree, new (untracked) files included.
    WorkingTree { rev: String },
}

impl DiffSpec {
    fn revisions(&self) -> Vec<&str> {
        match self {
            DiffSpec::Range { from, to, .. } => vec![from.as_str(), to.as_str()],
            DiffSpec::WorkingTree { rev } => vec![rev.as_str()],
        }
    }
}

/// Parse `--diff` (syntax only): `a..b` or `a...b` with both sides revision tokens, or one
/// revision token. `Err` is the refusal text.
pub fn parse_diff_spec(spec: &str) -> Result<DiffSpec, String> {
    let s = spec.trim();
    let bad = || {
        format!(
            "-Diff '{}' must be a range base..head or base...head, or one revision (diffed against the working tree, new files included); no spaces or control characters, no side starting with '-'.",
            spec.escape_debug()
        )
    };
    if s.is_empty() || s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad());
    }
    let sep = if s.contains("...") {
        "..."
    } else if s.contains("..") {
        ".."
    } else {
        return if rev_token_ok(s) {
            Ok(DiffSpec::WorkingTree { rev: s.to_string() })
        } else {
            Err(bad())
        };
    };
    let (a, b) = s.split_once(sep).ok_or_else(bad)?;
    if !rev_token_ok(a) || !rev_token_ok(b) || b.starts_with('.') {
        return Err(bad());
    }
    Ok(DiffSpec::Range {
        from: a.to_string(),
        sep,
        to: b.to_string(),
    })
}

/// [`parse_diff_spec`] plus: every revision it names is a commit git knows in `repo`.
pub fn check_diff_spec(repo: &Path, spec: &str) -> Result<DiffSpec, String> {
    let d = parse_diff_spec(spec)?;
    for r in d.revisions() {
        if let Err(why) = worktree::resolve_commit(repo, r) {
            return Err(format!("-Diff '{spec}': {why}; nothing was started."));
        }
    }
    Ok(d)
}

/// The `--diff` patch of a run: its range, where it is written, how the prompt names it, and -
/// once written - its sha256, file count, size and redaction count.
#[derive(Debug, Clone, Default)]
pub struct DiffPlan {
    pub range: String,
    /// The task's handoffs directory the patch is written into.
    pub dir: PathBuf,
    /// (R5, F19-3) The file name before its content hash: the written file is
    /// `<stem>.<sha8>.patch` ([`patch_stem`]).
    pub stem: String,
    /// The written file (empty until [`write_patch`] ran).
    pub path: PathBuf,
    /// The path the prompt names: repo-relative (`/`-separated) when the collab directory lies in
    /// the repository, else absolute. Before the write it shows `<sha8>` in place of the hash.
    pub rel: String,
    pub sha256: String,
    pub files: i64,
    pub bytes: u64,
    /// (R5, F19-2) How many secret-shaped values the redactor replaced in the patch.
    pub redactions: usize,
    /// (0.2.5 R8, F18-4 / F19-2 / F20-6) `--diff-untracked`: a one-revision patch carries the
    /// untracked files too. By default it holds the tracked changes only (`--diff-tracked-only`
    /// names that default).
    pub include_untracked: bool,
    /// (0.2.5 D1) The untracked (not ignored, not excluded) files a one-revision patch carries, in
    /// git's order - every one of them, its content included in the patch. Empty for a range and
    /// without `--diff-untracked`.
    pub untracked: Vec<String>,
    /// (0.2.5 R8) The untracked (not ignored, not excluded) files a one-revision patch LEFT OUT (the
    /// default), in git's order. Empty for a range; (0.2.5 D5) under `--diff-untracked` the files
    /// git could not read ([`render_patch_with`]).
    pub untracked_left_out: Vec<String>,
    /// (0.2.5 D1, F28-3) How many byte sequences of git's output were not valid UTF-8 and were
    /// replaced (U+FFFD) before the redaction: the patch is not byte-exact there.
    pub lossy: usize,
}

impl DiffPlan {
    /// Whether the patch was written (its sha256 is known).
    pub fn written(&self) -> bool {
        !self.sha256.is_empty()
    }
}

/// The patch's file-name stem in the task's handoffs (the written name is
/// `<stem>.<sha8>.patch`, [`write_patch`]): next to a brief named `<NN>-<prefix>-<slug>.md` in
/// those handoffs, the brief's own stem (`<NN>-<prefix>-<slug>`); otherwise `<nn>-<prefix>-diff`
/// with the run's handoff number (a panel member: the panel's first, so members share it) and the
/// coordinator's brief prefix. A brief named like a reply (`04-codex-reply.md`) is not taken as a
/// stem.
pub fn patch_stem(
    brief_file: Option<&str>,
    brief_in_handoffs: bool,
    nn: u32,
    prefix: &str,
) -> String {
    static CONV: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let conv =
        CONV.get_or_init(|| regex::Regex::new(r"^(\d{2,})-([a-z][a-z0-9]*)-.+\.md$").unwrap());
    if brief_in_handoffs {
        if let Some(f) = brief_file {
            if let Some(c) = conv.captures(f) {
                let first = &c[2];
                if !super::args::reply_prefixes().contains(&first) {
                    if let Some(stem) = f.strip_suffix(".md") {
                        return stem.to_string();
                    }
                }
            }
        }
    }
    format!("{nn:02}-{prefix}-diff")
}

/// The repo-relative (`/`) form of `path`, else its absolute form.
fn shown_path(repo_root: &Path, path: &Path) -> String {
    c3_core::paths::repo_relative(repo_root, path)
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// Plan the `--diff` patch (nothing is written): its stem ([`patch_stem`]) in `handoffs_dir`.
pub fn plan_diff(
    repo_root: &Path,
    handoffs_dir: &Path,
    brief_path: Option<&Path>,
    nn: u32,
    prefix: &str,
    spec: &str,
) -> DiffPlan {
    let in_handoffs = brief_path
        .and_then(|b| b.parent())
        .and_then(|d| std::fs::canonicalize(d).ok())
        .zip(std::fs::canonicalize(handoffs_dir).ok())
        .is_some_and(|(a, b)| a == b);
    let stem = patch_stem(
        brief_path
            .and_then(|b| b.file_name())
            .and_then(|n| n.to_str()),
        in_handoffs,
        nn,
        prefix,
    );
    // the name the prompt shows until the write knows the hash (a dry run shows this one)
    let rel = format!(
        "{}/{stem}.<sha8>.patch",
        shown_path(repo_root, handoffs_dir).trim_end_matches(['/', '\\'])
    );
    DiffPlan {
        range: spec.trim().to_string(),
        dir: handoffs_dir.to_path_buf(),
        stem,
        rel,
        ..Default::default()
    }
}

/// The exclude pathspecs of a patch: `.collab`, C3's folder `.c3` and its legacy name `.eck` (both
/// compared the way the file system compares names - case-insensitively on Windows and macOS, so
/// with git's `icase` magic there - and, 0.2.5 S2 F36-8, at any depth, as packs and snapshots drop
/// them: `**/.c3/**` with git's `glob` magic), and the collab directory (when it lies in the
/// repository under another name).
pub fn diff_excludes(repo_root: &Path, collab_root: &Path) -> Vec<String> {
    let magic = if cfg!(any(windows, target_os = "macos")) {
        "exclude,glob,icase"
    } else {
        "exclude,glob"
    };
    let mut paths = vec![".collab".to_string()];
    if let Some(r) = c3_core::paths::repo_relative(repo_root, collab_root) {
        if !r.is_empty() && !paths.contains(&r) {
            paths.push(r);
        }
    }
    let mut v: Vec<String> = paths.iter().map(|p| format!(":(exclude){p}")).collect();
    for state in [
        crate::state_dir::STATE_DIR,
        crate::state_dir::LEGACY_STATE_DIR,
    ] {
        v.push(format!(":({magic})**/{state}/**"));
    }
    v
}

/// Run git in `repo` (optionally with another index file and stdin bytes); stdout on exit 0.
fn git_out(
    repo: &Path,
    args: &[&str],
    index: Option<&Path>,
    stdin: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    use std::io::Write;
    // (R5, F18-2) no optional lock on the live repository
    let mut cmd = worktree::git_cmd(repo);
    cmd.args(args);
    if let Some(i) = index {
        cmd.env("GIT_INDEX_FILE", i);
    }
    cmd.stdin(if stdin.is_some() {
        std::process::Stdio::piped()
    } else {
        std::process::Stdio::null()
    });
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if let (Some(bytes), Some(mut sin)) = (stdin, child.stdin.take()) {
        let _ = sin.write_all(bytes);
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}

/// (R5, F19-2) The paths of `paths` (repo-relative, `/`) the repository excludes from what may leave
/// the machine: matched by its ignore rules (`git check-ignore --no-index`: `.gitignore` files,
/// `.git/info/exclude` and the global excludes - a TRACKED file included) or by a `.c3ignore` file
/// in its directory or any directory above it (gitignore syntax, the deeper file deciding - as
/// `c3 pack` reads it).
fn ignore_excluded(
    repo: &Path,
    paths: &[String],
) -> Result<std::collections::HashSet<String>, String> {
    use std::io::Write;
    let mut out: std::collections::HashSet<String> = std::collections::HashSet::new();
    if paths.is_empty() {
        return Ok(out);
    }
    let mut input: Vec<u8> = Vec::new();
    for p in paths {
        input.extend_from_slice(p.as_bytes());
        input.push(0);
    }
    let mut child = worktree::git_cmd(repo)
        .args(["check-ignore", "--no-index", "-z", "--stdin"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("git check-ignore: {e}"))?;
    if let Some(mut sin) = child.stdin.take() {
        let _ = sin.write_all(&input);
    }
    let o = child
        .wait_with_output()
        .map_err(|e| format!("git check-ignore: {e}"))?;
    // exit 0: some path is ignored, 1: none is; anything else is an error
    match o.status.code() {
        Some(0) | Some(1) => {}
        _ => {
            return Err(format!(
                "git check-ignore failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            ))
        }
    }
    for p in String::from_utf8_lossy(&o.stdout).split('\0') {
        if !p.is_empty() {
            out.insert(p.to_string());
        }
    }
    // `.c3ignore` (or `.eckignore` in a directory without one): per directory, from the
    // repository root down (a deeper file decides) - the reader discovery uses too
    let mut context_ignore = crate::state_dir::ContextIgnore::new(repo);
    for p in paths {
        if out.contains(p) {
            continue;
        }
        if context_ignore.excluded(p, false) {
            out.insert(p.clone());
        }
    }
    Ok(out)
}

/// The NUL-separated paths of a `-z` listing.
fn nul_list(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// A rendered `--diff` patch: its bytes (as git wrote them), the untracked files a one-revision
/// patch carries (only under `--diff-untracked`) and - (0.2.5 R8) by default - the untracked files it
/// left out ((0.2.5 D5) under `--diff-untracked`: the ones git could not read). Both lists are empty
/// for a range.
#[derive(Debug, Clone, Default)]
pub struct RenderedPatch {
    pub bytes: Vec<u8>,
    pub untracked: Vec<String>,
    pub untracked_left_out: Vec<String>,
}

/// [`render_patch_with`] with the untracked files included, its bytes only.
pub fn render_patch(repo: &Path, spec: &DiffSpec, excludes: &[String]) -> Result<Vec<u8>, String> {
    render_patch_with(repo, spec, excludes, true).map(|p| p.bytes)
}

/// The patch of `spec` in `repo`, the `excludes` left out. A range is `git diff <range>`; one
/// revision is diffed against the working tree with the untracked (not ignored) files marked
/// intent-to-add in a TEMPORARY copy of the index when `include_untracked` (`--diff-untracked`), so
/// new files are in the patch and the real index is never touched; (0.2.5 R8, F18-4 / F19-2 /
/// F20-6) by default no untracked file is marked and the patch holds the tracked changes only. The
/// untracked files the patch carries - or, by default, the ones it left out - are returned beside
/// it. (R5, F19-2) A file the repository's ignore rules or a
/// `.c3ignore` exclude ([`ignore_excluded`]) never enters the patch - an untracked one is not
/// marked, a tracked one is hidden ([`hide_in_index`]). (R5, F19-5) The revision or range is passed
/// after `--end-of-options`, so git never reads it as an option.
///
/// (0.2.5 D1, F28-2) However many changed paths are excluded, the review is not refused: they never
/// reach git's command line. They go through a temporary index instead (fed on stdin): a range is
/// diffed as `git diff --cached <base>` against an index of its `to` side in which every excluded
/// path carries its `base` version (or is absent), so it shows no change; a one-revision diff's
/// temporary index gives each excluded path the revision's version marked skip-worktree. With
/// nothing excluded the command is the plain `git diff` it always was.
///
/// (0.2.5 D5) Under `--diff-untracked` an untracked file git cannot read (a virus scanner took it
/// away or holds it, another process holds it locked) does not fail the patch: when the batch of
/// all untracked files fails
/// and the tracked changes alone render, the files are tried one by one ([`split_unreadable`]), the
/// ones git cannot read are returned as left out, and the patch carries the rest.
pub fn render_patch_with(
    repo: &Path,
    spec: &DiffSpec,
    excludes: &[String],
    include_untracked: bool,
) -> Result<RenderedPatch, String> {
    let mut pathspec: Vec<String> = vec!["--".into(), ".".into()];
    pathspec.extend(excludes.iter().cloned());
    let args_of = |head: &[&str], what: &str| -> Vec<String> {
        let mut v: Vec<String> = ["-c", "core.quotepath=false", "diff"]
            .iter()
            .chain(head.iter())
            .map(|s| s.to_string())
            .collect();
        v.push("--end-of-options".into());
        v.push(what.to_string());
        v.extend(pathspec.iter().cloned());
        v
    };
    let run = |args: &[String], index: Option<&Path>| -> Result<Vec<u8>, String> {
        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        git_out(repo, &refs, index, None)
    };
    // the changed paths of `what` (diffed with `head`) the repository excludes, sorted
    let hidden_of =
        |head: &[&str], what: &str, index: Option<&Path>| -> Result<Vec<String>, String> {
            let mut h: Vec<&str> = head.to_vec();
            h.extend(["--name-only", "-z", "--no-ext-diff"]);
            let changed = nul_list(&run(&args_of(&h, what), index)?);
            let mut hidden: Vec<String> = ignore_excluded(repo, &changed)?.into_iter().collect();
            hidden.sort();
            Ok(hidden)
        };
    match spec {
        DiffSpec::Range { from, sep, to } => {
            let range = format!("{from}{sep}{to}");
            let hidden = hidden_of(&[], &range, None)?;
            if hidden.is_empty() {
                let bytes = run(&args_of(&["--no-color", "--no-ext-diff"], &range), None)?;
                return Ok(RenderedPatch {
                    bytes,
                    ..Default::default()
                });
            }
            // (F28-2) `from..to` is `base` (from, or the merge base for `...`) against `to`: the
            // index of `to` with every excluded path put back to its `base` version
            let to_sha = worktree::resolve_commit(repo, to)?;
            let from_sha = worktree::resolve_commit(repo, from)?;
            let base = if *sep == "..." {
                String::from_utf8_lossy(&git_out(
                    repo,
                    &["merge-base", &from_sha, &to_sha],
                    None,
                    None,
                )?)
                .trim()
                .to_string()
            } else {
                from_sha
            };
            let tmp = temp_index_path();
            let result = (|| -> Result<Vec<u8>, String> {
                git_out(repo, &["read-tree", &to_sha], Some(&tmp), None)?;
                hide_in_index(repo, &tmp, &base, &hidden, false)?;
                run(
                    &args_of(&["--cached", "--no-color", "--no-ext-diff"], &base),
                    Some(&tmp),
                )
            })();
            remove_temp_index(&tmp);
            Ok(RenderedPatch {
                bytes: result?,
                ..Default::default()
            })
        }
        DiffSpec::WorkingTree { rev } => {
            let index_rel = String::from_utf8_lossy(&git_out(
                repo,
                &["rev-parse", "--git-path", "index"],
                None,
                None,
            )?)
            .trim()
            .to_string();
            let index = {
                let p = PathBuf::from(&index_rel);
                if p.is_absolute() {
                    p
                } else {
                    repo.join(p)
                }
            };
            // a fresh temporary copy of the real index (nothing when the repository has none yet)
            let fresh_index = || -> Result<PathBuf, String> {
                let tmp = temp_index_path();
                if index.is_file() {
                    std::fs::copy(&index, &tmp)
                        .map_err(|e| format!("the index could not be copied ({e})"))?;
                }
                Ok(tmp)
            };
            // the patch against the working tree with the files `carried` marked intent-to-add, in
            // a temporary index of its own
            let render = |carried: &[String]| -> Result<Vec<u8>, String> {
                let tmp = fresh_index()?;
                let result = (|| -> Result<Vec<u8>, String> {
                    if !carried.is_empty() {
                        let mut marked: Vec<u8> = Vec::new();
                        for p in carried {
                            marked.extend_from_slice(p.as_bytes());
                            marked.push(0);
                        }
                        git_out(
                            repo,
                            &[
                                "--literal-pathspecs",
                                "add",
                                "--intent-to-add",
                                "--pathspec-from-file=-",
                                "--pathspec-file-nul",
                            ],
                            Some(&tmp),
                            Some(&marked),
                        )?;
                    }
                    let hidden = hidden_of(&[], rev, Some(&tmp))?;
                    if !hidden.is_empty() {
                        let base = worktree::resolve_commit(repo, rev)?;
                        hide_in_index(repo, &tmp, &base, &hidden, true)?;
                    }
                    run(&args_of(&["--no-color", "--no-ext-diff"], rev), Some(&tmp))
                })();
                remove_temp_index(&tmp);
                result
            };
            let tmp = fresh_index()?;
            let listed = (|| -> Result<Vec<String>, String> {
                let mut ls: Vec<String> = vec![
                    "ls-files".into(),
                    "--others".into(),
                    "--exclude-standard".into(),
                    "-z".into(),
                ];
                ls.extend(pathspec.iter().cloned());
                let ls_refs: Vec<&str> = ls.iter().map(|s| s.as_str()).collect();
                let untracked = nul_list(&git_out(repo, &ls_refs, Some(&tmp), None)?);
                // (R5, F19-2) an untracked file is a change - unless a `.c3ignore` excludes it
                // (`--exclude-standard` already left the git-ignored ones out)
                let hidden = ignore_excluded(repo, &untracked)?;
                Ok(untracked
                    .into_iter()
                    .filter(|p| !hidden.contains(p))
                    .collect())
            })();
            remove_temp_index(&tmp);
            let found = listed?;
            // (0.2.5 R8) carried only when asked; by default left out (and named)
            if !include_untracked {
                return Ok(RenderedPatch {
                    bytes: render(&[])?,
                    untracked: Vec::new(),
                    untracked_left_out: found,
                });
            }
            let failure = match render(&found) {
                Ok(bytes) => {
                    return Ok(RenderedPatch {
                        bytes,
                        untracked: found,
                        untracked_left_out: Vec::new(),
                    })
                }
                Err(e) => e,
            };
            // (0.2.5 D5) the batch failed: when the tracked changes alone render, an untracked file
            // git cannot read made it fail - each file is tried on its own, the ones git cannot read
            // are left out, and the patch is made with the rest
            if found.is_empty() || render(&[]).is_err() {
                return Err(failure);
            }
            let (readable, unreadable) = split_unreadable(repo, rev, &found, fresh_index()?);
            Ok(RenderedPatch {
                bytes: render(&readable)?,
                untracked: readable,
                untracked_left_out: unreadable,
            })
        }
    }
}

/// (0.2.5 D5) The untracked files `paths` split into the ones git can read and the ones it cannot,
/// each tried ON ITS OWN in the temporary index `probe` (removed afterwards): marked intent-to-add,
/// then its own patch against `rev` rendered - a file whose mark or patch fails (a file a virus
/// scanner took away or holds, a file another process holds locked, a file gone meanwhile) is one
/// git cannot read. Only called after the batch of all of them failed.
fn split_unreadable(
    repo: &Path,
    rev: &str,
    paths: &[String],
    probe: PathBuf,
) -> (Vec<String>, Vec<String>) {
    let mut readable = Vec::new();
    let mut unreadable = Vec::new();
    for p in paths {
        let read = git_out(
            repo,
            &["--literal-pathspecs", "add", "--intent-to-add", "--", p],
            Some(&probe),
            None,
        )
        .and_then(|_| {
            git_out(
                repo,
                &[
                    "--literal-pathspecs",
                    "-c",
                    "core.quotepath=false",
                    "diff",
                    "--no-color",
                    "--no-ext-diff",
                    "--end-of-options",
                    rev,
                    "--",
                    p,
                ],
                Some(&probe),
                None,
            )
        })
        .is_ok();
        if read {
            readable.push(p.clone());
        } else {
            unreadable.push(p.clone());
        }
    }
    remove_temp_index(&probe);
    (readable, unreadable)
}

/// A fresh temporary index file name (nothing created).
fn temp_index_path() -> PathBuf {
    std::env::temp_dir().join(format!("c3-diff-index-{}", uuid::Uuid::new_v4().simple()))
}

/// Remove a temporary index and its lock file (best effort).
fn remove_temp_index(tmp: &Path) {
    let _ = std::fs::remove_file(tmp);
    let _ = std::fs::remove_file(tmp.with_extension("lock"));
}

/// (0.2.5 D1, F28-2) Hide the `hidden` paths of a patch in the temporary `index`, bounded however
/// many there are (every list goes to git on stdin, never on the command line): each one present
/// in the commit `base` gets `base`'s version (mode and object, from `git ls-tree -r -z`), each one
/// absent there is removed (`git update-index -z --index-info`), so the index shows no change for
/// it against `base`. `skip_worktree` (a diff against the working tree) also marks the kept ones
/// skip-worktree, so their working-tree content is not read either.
fn hide_in_index(
    repo: &Path,
    index: &Path,
    base: &str,
    hidden: &[String],
    skip_worktree: bool,
) -> Result<(), String> {
    let want: std::collections::HashSet<&str> = hidden.iter().map(|s| s.as_str()).collect();
    let listing = git_out(
        repo,
        &["ls-tree", "-r", "-z", "--full-tree", base],
        None,
        None,
    )?;
    let mut present: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    for rec in listing.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let rec = String::from_utf8_lossy(rec);
        // `<mode> SP <type> SP <object> TAB <path>`
        let Some((meta, path)) = rec.split_once('\t') else {
            continue;
        };
        if !want.contains(path) {
            continue;
        }
        let mut parts = meta.split(' ');
        if let (Some(mode), Some(_), Some(object)) = (parts.next(), parts.next(), parts.next()) {
            present.insert(path.to_string(), (mode.to_string(), object.to_string()));
        }
    }
    let zero = "0".repeat(base.len().max(40));
    let mut info: Vec<u8> = Vec::new();
    let mut kept: Vec<u8> = Vec::new();
    for p in hidden {
        match present.get(p) {
            Some((mode, object)) => {
                info.extend_from_slice(format!("{mode} {object}\t{p}").as_bytes());
                kept.extend_from_slice(p.as_bytes());
                kept.push(0);
            }
            None => info.extend_from_slice(format!("0 {zero}\t{p}").as_bytes()),
        }
        info.push(0);
    }
    git_out(
        repo,
        &["update-index", "-z", "--index-info"],
        Some(index),
        Some(&info),
    )?;
    if skip_worktree && !kept.is_empty() {
        git_out(
            repo,
            &["update-index", "-z", "--skip-worktree", "--stdin"],
            Some(index),
            Some(&kept),
        )?;
    }
    Ok(())
}

/// (0.2.5 D4) The test hook that makes the patch placement behave as on a file system without hard
/// links (`CODEX_CONSULT_TEST_PATCH_NO_HARDLINK`, any value): the placement by one rename is then
/// proved.
pub(crate) const PATCH_NO_HARDLINK_HOOK: &str = "CODEX_CONSULT_TEST_PATCH_NO_HARDLINK";

/// (0.2.5 R8, F53-4 / F43-7) From this age (seconds since its last change) a write-temp file of C3's
/// own patch pattern in a handoffs directory ([`patch_temp_name`]) is the debris of a placement that
/// was killed between its write and its placement: it is removed when a patch is placed there.
pub const PATCH_TEMP_STALE_SECS: u64 = 24 * 60 * 60;

/// What the patch name holds when a placement looks at it ([`judge_patch_name`]).
enum AtName {
    /// Nothing: the patch is published there.
    Missing,
    /// A regular file holding exactly this patch's bytes: reused.
    Same,
    /// (0.2.5 R8) An EMPTY regular file - the name claim of an earlier build's placement: replaced
    /// by the rename.
    Empty,
}

/// (0.2.5 R8, F51-3 / F52-2 / F53-6) What is at the patch name `leaf` of `dir`, read through a
/// handle opened without following it. The name is the patch's content (`<stem>.<sha8>.patch`), so a
/// regular file there holding other bytes is not this patch: an `Err` ("holds other bytes"), never
/// overwritten - as is anything that is not a regular file (a link, a directory).
fn judge_patch_name(
    dir: &cap_std::fs::Dir,
    leaf: &std::ffi::OsStr,
    bytes: &[u8],
    shown: &str,
) -> Result<AtName, String> {
    use crate::contain::Step;
    match crate::contain::read_file_in(dir, leaf) {
        Ok(b) if b == bytes => Ok(AtName::Same),
        Ok(b) if b.is_empty() => Ok(AtName::Empty),
        Ok(_) => Err(format!(
            "{shown} already holds other bytes; a patch file is never overwritten"
        )),
        Err(Step::Missing) => Ok(AtName::Missing),
        Err(Step::Link { .. }) | Err(Step::NotFile) => Err(format!(
            "{shown} exists and is not a regular file; it is never reused or overwritten"
        )),
        Err(e) => Err(format!("{shown} cannot be read ({e})")),
    }
}

/// (0.2.5 R8, F53-4 / F43-7) Whether `name` is a write-temp name of C3's own patch pattern:
/// `<stem>.<sha8>.<32 hex>.tmp` ([`patch_temp_name`]).
fn is_patch_temp_name(name: &str) -> bool {
    let hex = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit());
    let Some(rest) = name.strip_suffix(".tmp") else {
        return false;
    };
    let mut parts = rest.rsplitn(3, '.');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(id), Some(sha8), Some(stem)) => !stem.is_empty() && hex(id, 32) && hex(sha8, 8),
        _ => false,
    }
}

/// (0.2.5 R8, F53-4 / F43-7) Remove from the handoffs directory `dir` (through its handle, regular
/// files only, best effort) every write-temp file of C3's own patch pattern ([`is_patch_temp_name`])
/// unchanged for [`PATCH_TEMP_STALE_SECS`] or more: a placement killed between its write and its
/// placement left it, and nothing else ever removes it. (0.2.5 D5) The sweep is the one the project
/// folder's writer uses too ([`crate::contain::sweep_stale_temps`]).
fn sweep_stale_patch_temps(dir: &cap_std::fs::Dir) {
    crate::contain::sweep_stale_temps(dir, is_patch_temp_name, PATCH_TEMP_STALE_SECS);
}

/// (0.2.5 D1, F43-1) The task's handoffs directory `dir_path`, opened - and what is missing of it
/// created - through handles ([`crate::contain`]): from a root on the repository when it lies inside
/// it (as the collab directory does by default), else from a root on the collab directory. Every
/// component below that root is opened without following it, so a link anywhere above the handoffs
/// directory refuses the placement; nothing is created by path name (only an outside collab root
/// itself, the anchor, is created when it is missing).
fn open_patch_dir(
    dir_path: &Path,
    repo_root: &Path,
    collab_root: &Path,
) -> Result<cap_std::fs::Dir, String> {
    let shown = dir_path.display();
    let (anchor, rel) =
        match c3_core::paths::repo_relative(repo_root, dir_path).filter(|r| !r.is_empty()) {
            Some(rel) => (repo_root, rel),
            None => match c3_core::paths::repo_relative(collab_root, dir_path)
                .filter(|r| !r.is_empty())
            {
                Some(rel) => {
                    if !collab_root.exists() {
                        let _ = std::fs::create_dir_all(collab_root);
                    }
                    (collab_root, rel)
                }
                None => {
                    return Err(format!(
                        "{shown} lies neither in the repository nor in the collab directory"
                    ))
                }
            },
        };
    let root = crate::contain::Root::open_anchor(anchor)
        .map_err(|e| format!("{} {}", anchor.display(), e.words()))?;
    crate::contain::race_point("patch-dir");
    root.open_dir(Path::new(&rel), true).map_err(|e| {
        format!(
            "{shown} cannot be reached through {}: `{}` {}",
            anchor.display(),
            e.at.display(),
            e.why
        )
    })
}

/// (R5, F19-3) Put `bytes` at `dir/name` without ever overwriting a file: an existing regular file
/// with the SAME bytes is reused (a panel sibling, or an earlier run, rendered the same patch); one
/// with other bytes, or anything that is not a regular file (a link, a directory), is an `Err`.
///
/// (0.2.5 R8, F51-3 / F52-2 / F53-6) The name IS the content (`<stem>.<sha8>.patch`), so there is no
/// claim protocol: the bytes go into a new temporary file in the handoffs directory (an exclusive
/// create relative to the directory's handle, synced); then the name is judged
/// ([`judge_patch_name`]) - the same bytes: the temporary file is removed and the patch reused;
/// other bytes: refused, nothing overwritten; nothing there: published by a hard link relative to
/// the handle where the file system has hard links, else by ONE atomic rename of the complete
/// temporary file onto the name (two runs with equal bytes may both do it - the result is the same
/// file). An EMPTY file at the name (an earlier build's claim) is replaced by that rename. The name is
/// never removed by name, and never holds partial bytes: on every error path the temporary file is
/// removed again, and a removal that fails is named in the `Err` (or, when the patch itself is in
/// place, said on stderr). Stale write-temp files of C3's own pattern are swept first
/// ([`sweep_stale_patch_temps`]).
///
/// (0.2.5 D1, F43-1) The handoffs directory is reached through handles from the start
/// ([`open_patch_dir`]).
fn place_patch(
    dir_path: &Path,
    repo_root: &Path,
    collab_root: &Path,
    name: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let dir = open_patch_dir(dir_path, repo_root, collab_root)?;
    place_patch_in(&dir, dir_path, name, bytes)
}

/// How often [`rename_onto`] tries, and how long it waits between tries.
const RENAME_TRIES: u32 = 6;
const RENAME_PAUSE: std::time::Duration = std::time::Duration::from_millis(50);

/// The one rename of a patch's temporary file onto its name, relative to the directory handle.
/// On Windows a rename onto a name that a virus scanner or an indexer holds open for an instant
/// fails with "access denied" (seen on 2026-11-24 on a file written a moment before): that one
/// failure is tried again [`RENAME_TRIES`] times, [`RENAME_PAUSE`] apart, before it is reported.
/// Every other error is returned at once.
fn rename_onto(
    dir: &cap_std::fs::Dir,
    tmp: &std::ffi::OsStr,
    leaf: &std::ffi::OsStr,
) -> std::io::Result<()> {
    let mut last = None;
    for attempt in 0..RENAME_TRIES {
        if attempt > 0 {
            std::thread::sleep(RENAME_PAUSE);
        }
        match dir.rename(tmp, dir, leaf) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("the rename was not tried")))
}

/// [`place_patch`] in the open handoffs directory `dir` (`dir_path` names it in messages).
fn place_patch_in(
    dir: &cap_std::fs::Dir,
    dir_path: &Path,
    name: &str,
    bytes: &[u8],
) -> Result<(), String> {
    use std::ffi::OsStr;
    let shown = dir_path.join(name).display().to_string();
    let leaf = OsStr::new(name);
    sweep_stale_patch_temps(dir);
    let tmp_name = patch_temp_name(name, &uuid::Uuid::new_v4().simple().to_string());
    let tmp = OsStr::new(&tmp_name);
    let remove_tmp = || match dir.remove_file(tmp) {
        Ok(()) => None,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => Some(e),
    };
    let noted = |msg: String, cleanup: Option<std::io::Error>| match cleanup {
        None => msg,
        Some(e) => format!("{msg}; its temporary file {tmp_name} could not be removed ({e})"),
    };
    let warn_left = |cleanup: Option<std::io::Error>| {
        if let Some(e) = cleanup {
            eprintln!(
                "codex-consult: the patch {shown} is in place, but its temporary file {tmp_name} could not be removed ({e})"
            );
        }
    };
    {
        let mut o = cap_std::fs::OpenOptions::new();
        o.write(true).create_new(true);
        let mut f = dir
            .open_with(tmp, &o)
            .map_err(|e| format!("{shown} could not be written ({e})"))?;
        let w = crate::contain::write_hook(&mut f, bytes).and_then(|()| f.sync_all());
        drop(f);
        if let Err(e) = w {
            return Err(noted(
                format!("{shown} could not be written ({e})"),
                remove_tmp(),
            ));
        }
    }
    // a name taken between the look and the hard link is judged again
    for _ in 0..3 {
        let at = match judge_patch_name(dir, leaf, bytes, &shown) {
            Ok(a) => a,
            Err(m) => return Err(noted(m, remove_tmp())),
        };
        crate::contain::race_point("patch-place");
        let renamed = match at {
            AtName::Same => {
                warn_left(remove_tmp());
                return Ok(());
            }
            // an earlier build's empty claim: replaced by the one rename
            AtName::Empty => rename_onto(dir, tmp, leaf),
            AtName::Missing => {
                let linked = if c3_core::test_hooks::hook(PATCH_NO_HARDLINK_HOOK).is_some() {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "hard links refused (test hook)",
                    ))
                } else {
                    dir.hard_link(tmp, dir, leaf)
                };
                match linked {
                    Ok(()) => {
                        warn_left(remove_tmp());
                        return Ok(());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    // no hard links on this file system: one atomic rename of the whole file.
                    // The limit: the look above and this rename are two steps, and a rename
                    // replaces - a foreign file with OTHER bytes that appears at this
                    // content-addressed name between them is replaced (a sibling's file has the
                    // same bytes). With hard links the name is judged again instead.
                    Err(_) => rename_onto(dir, tmp, leaf),
                }
            }
        };
        return match renamed {
            Ok(()) => Ok(()),
            // a sibling with the same bytes published the name meanwhile (and may hold it open)
            Err(_) if matches!(judge_patch_name(dir, leaf, bytes, &shown), Ok(AtName::Same)) => {
                warn_left(remove_tmp());
                Ok(())
            }
            Err(e) => Err(noted(
                format!("{shown} could not be placed ({e})"),
                remove_tmp(),
            )),
        };
    }
    Err(noted(
        format!("{shown} could not be placed: the name kept changing while it was placed"),
        remove_tmp(),
    ))
}

/// (0.2.5 D4) The write-temp name of the patch file `name` (`<stem>.<sha8>.patch`):
/// `<stem>.<sha8>.<uuid>.tmp`, `uuid` 32 lowercase hex digits.
fn patch_temp_name(name: &str, uuid: &str) -> String {
    format!("{}.{uuid}.tmp", name.trim_end_matches(".patch"))
}

/// (0.2.5 D1, F27-3) Whether the collab-relative path `key` (`<task>/handoffs/<file>`, `/`) is
/// the run's OWN patch: exactly the file [`write_patch`] wrote, or its write-temp name
/// ([`patch_temp_name`]: the written name without `.patch`, a dot, 32 hex digits, `.tmp`) - nothing
/// else that shares the patch's stem. Compared case-insensitively (the collab compare's way).
/// `false` before the patch was written.
pub fn is_run_patch_file(plan: &DiffPlan, task: &str, key: &str) -> bool {
    let Some(name) = plan.path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if !plan.written() {
        return false;
    }
    let key = key.to_ascii_lowercase();
    let dir = format!("{}/handoffs/", task.to_ascii_lowercase());
    let Some(file) = key.strip_prefix(&dir) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    if file == name {
        return true;
    }
    let stem = format!("{}.", name.trim_end_matches(".patch"));
    file.strip_prefix(&stem)
        .and_then(|rest| rest.strip_suffix(".tmp"))
        .is_some_and(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// (0.2.5 D1, F28-3) How many byte sequences of `raw` are not valid UTF-8 - each becomes one U+FFFD
/// in [`String::from_utf8_lossy`].
pub fn invalid_utf8_sequences(raw: &[u8]) -> usize {
    raw.utf8_chunks()
        .filter(|c| !c.invalid().is_empty())
        .count()
}

/// How many untracked files the operator notice and the ledger name.
pub const UNTRACKED_NAMED: usize = 10;

/// (0.2.5 D5) Why a `--diff-untracked` patch left an untracked file out (the ledger's
/// `untracked_left_out_reason`).
pub const UNREADABLE_REASON: &str = "git could not read it";

/// (0.2.5 D1, F18-4 / F19-2 / F20-6) Untracked file names as the operator notice, the handoff and
/// the ledger name them: at most [`UNTRACKED_NAMED`] names, each as data (in backticks; (0.2.5 R8,
/// F53-5) a name with a backtick, a control character or a format character - category Cf,
/// [`crate::unicode::is_format_char`] - is counted instead), and how many more there are.
pub fn named_paths(paths: &[String]) -> (Vec<String>, usize) {
    let mut named: Vec<String> = Vec::new();
    let mut more = 0usize;
    for p in paths {
        if named.len() < UNTRACKED_NAMED
            && !p
                .chars()
                .any(|c| c == '`' || c.is_control() || crate::unicode::is_format_char(c))
        {
            named.push(p.clone());
        } else {
            more += 1;
        }
    }
    (named, more)
}

/// The untracked files a one-revision patch carries (under `--diff-untracked`), named as
/// [`named_paths`] names them.
pub fn untracked_named(plan: &DiffPlan) -> (Vec<String>, usize) {
    named_paths(&plan.untracked)
}

/// `` `a`, `b`, and 3 more `` - the list of [`named_paths`].
fn shown_list(named: &[String], more: usize) -> String {
    let mut list = named
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ");
    if more > 0 {
        if !list.is_empty() {
            list.push_str(", ");
        }
        list.push_str(&format!("and {more} more"));
    }
    list
}

/// The notice printed on stderr before the reviewer starts - and (0.2.5 R8) kept as a run warning,
/// so the handoff header and the ledger carry it too - when a one-revision patch carries untracked
/// files (`--diff-untracked`) or (0.2.5 R8, the default) left untracked files out: how many, up to
/// ten names (as data), the rest as a count. `None` when there are none either way. (0.2.5 D5)
/// Under `--diff-untracked` it names the files left out because git could not read them too.
pub fn untracked_notice(plan: &DiffPlan) -> Option<String> {
    let unreadable = (plan.include_untracked && !plan.untracked_left_out.is_empty()).then(|| {
        let (named, more) = named_paths(&plan.untracked_left_out);
        let list = if named.is_empty() {
            format!("{more} name(s) not shown")
        } else {
            shown_list(&named, more)
        };
        format!(
            "{} untracked file(s) were left out - git could not read them: {list}",
            plan.untracked_left_out.len()
        )
    });
    if !plan.untracked.is_empty() {
        let (named, more) = untracked_named(plan);
        let mut n = format!(
            "the -Diff patch includes {} untracked file(s) with their content (--diff-untracked; redacted only where a value looks like a secret): {}",
            plan.untracked.len(),
            shown_list(&named, more)
        );
        if let Some(u) = unreadable {
            n.push_str("; ");
            n.push_str(&u);
        }
        return Some(n);
    }
    if let Some(u) = unreadable {
        return Some(format!(
            "the -Diff patch holds the tracked changes only: {u}"
        ));
    }
    if !plan.untracked_left_out.is_empty() {
        let (named, more) = named_paths(&plan.untracked_left_out);
        return Some(format!(
            "the -Diff patch holds the tracked changes only: {} untracked file(s) were left out: {} - pass --diff-untracked to include them",
            plan.untracked_left_out.len(),
            shown_list(&named, more)
        ));
    }
    None
}

/// Write the `--diff` patch, filling in its name, sha256, file count, size and redaction count.
/// (R5, F19-2) The patch passes through the redactor packs for remote models use
/// ([`crate::pack::redact`]): a secret-shaped value is replaced by its marker, the rest is kept, and
/// the sha256 is of the file as written. (R5, F19-3) The file is `<stem>.<sha8>.patch` - one file
/// per content, never overwritten ([`place_patch`]): panel members that render the same bytes
/// share it, a member that renders other bytes gets its own. `Err` is the capability failure the
/// run records.
pub fn write_patch(
    plan: &mut DiffPlan,
    repo_root: &Path,
    collab_root: &Path,
) -> Result<(), String> {
    let spec = parse_diff_spec(&plan.range)?;
    let rendered = render_patch_with(
        repo_root,
        &spec,
        &diff_excludes(repo_root, collab_root),
        plan.include_untracked,
    )
    .map_err(|e| format!("the -Diff patch could not be made: {e}"))?;
    let raw = rendered.bytes;
    // (0.2.5 D1, F28-3) the redactor works on text: the conversion stays, and is recorded
    let lossy = invalid_utf8_sequences(&raw);
    let (safe, redactions) = crate::pack::redact::redact(&String::from_utf8_lossy(&raw));
    let bytes = safe.into_bytes();
    let sha = c3_core::sha256_hex(&bytes);
    let name = format!("{}.{}.patch", plan.stem, &sha[..8]);
    let path = plan.dir.join(&name);
    place_patch(&plan.dir, repo_root, collab_root, &name, &bytes)
        .map_err(|e| format!("the -Diff patch could not be written: {e}"))?;
    plan.rel = shown_path(repo_root, &path);
    plan.path = path;
    plan.sha256 = sha;
    plan.redactions = redactions;
    plan.lossy = lossy;
    plan.untracked = rendered.untracked;
    plan.untracked_left_out = rendered.untracked_left_out;
    plan.bytes = bytes.len() as u64;
    plan.files = String::from_utf8_lossy(&bytes)
        .lines()
        .filter(|l| l.starts_with("diff --git "))
        .count() as i64;
    Ok(())
}

/// The prompt line that names the patch: where it is, what was left out, that it is for reading
/// (not for applying) and - (R5, F19-2) when the redactor replaced any - how many secret-shaped
/// values were redacted.
pub fn diff_prompt_line(plan: &DiffPlan) -> String {
    let mut l = format!(
        "The change under review is the patch at `{}` (`git diff {}`; `.collab/`, `.c3/` and files the repository's ignore rules or `.c3ignore` exclude left out): read it first and review that change. The patch is for reading, not for applying",
        plan.rel, plan.range
    );
    if plan.redactions > 0 {
        l.push_str(&format!(
            ": {} secret-shaped value(s) in it were replaced by `[REDACTED:<kind>]` markers.",
            plan.redactions
        ));
    } else {
        l.push('.');
    }
    // (0.2.5 R8) the tracked changes only: say so, so an absent new file is not read as deleted;
    // (0.2.5 D5) under --diff-untracked the files left out are the ones git could not read
    if plan.include_untracked && !plan.untracked_left_out.is_empty() {
        l.push_str(&format!(
            " {} untracked file(s) of the working tree were left out of it (git could not read them).",
            plan.untracked_left_out.len()
        ));
    } else if !plan.untracked_left_out.is_empty() {
        l.push_str(&format!(
            " {} untracked file(s) of the working tree were left out of it (only tracked changes are in the patch).",
            plan.untracked_left_out.len()
        ));
    }
    // (0.2.5 D1, F28-3) git's output was not valid UTF-8 everywhere: the patch is not byte-exact
    if plan.lossy > 0 {
        l.push_str(&format!(
            " {} byte sequence(s) in git's output were not valid UTF-8 and were replaced by U+FFFD, so the patch is not byte-exact there.",
            plan.lossy
        ));
    }
    l
}

/// The ledger's `diff` record: `{range, patch, sha256, files, bytes, redactions}` (the sha256 of the
/// file as written). (0.2.5 D1) Plus `lossy_utf8: true` (and `lossy_utf8_sequences`) when git's
/// output was not valid UTF-8 everywhere (F28-3); `untracked` (up to ten names) and
/// `untracked_count` when a one-revision patch carries untracked files (`--diff-untracked`);
/// (0.2.5 R8) `untracked_left_out` (up to ten names) and `untracked_left_out_count` when it left
/// untracked files out (the default); (0.2.5 D5) under `--diff-untracked` those are the files git
/// could not read, and `untracked_left_out_reason` says so ([`UNREADABLE_REASON`]).
pub fn ledger_diff_value(plan: &DiffPlan) -> serde_json::Value {
    let mut v = serde_json::json!({
        "range": plan.range,
        "patch": plan.rel,
        "sha256": plan.sha256,
        "files": plan.files,
        "bytes": plan.bytes,
        "redactions": plan.redactions,
    });
    if plan.lossy > 0 {
        v["lossy_utf8"] = serde_json::json!(true);
        v["lossy_utf8_sequences"] = serde_json::json!(plan.lossy);
    }
    if !plan.untracked.is_empty() {
        let (named, _) = untracked_named(plan);
        v["untracked"] = serde_json::json!(named);
        v["untracked_count"] = serde_json::json!(plan.untracked.len());
    }
    if !plan.untracked_left_out.is_empty() {
        let (named, _) = named_paths(&plan.untracked_left_out);
        v["untracked_left_out"] = serde_json::json!(named);
        v["untracked_left_out_count"] = serde_json::json!(plan.untracked_left_out.len());
        if plan.include_untracked {
            v["untracked_left_out_reason"] = serde_json::json!(UNREADABLE_REASON);
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::worktree::git_capture;

    fn repo(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "c3-reviewed-{tag}-{}",
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
        std::fs::write(repo.join("app.txt"), "one\n").unwrap();
        git_capture(&repo, &["add", "-A"]).unwrap();
        git_capture(&repo, &["commit", "-q", "-m", "init"]).unwrap();
        repo
    }

    #[test]
    fn named_ranges_finds_git_ranges_and_not_prose_or_code() {
        assert_eq!(
            named_ranges("Review `git diff main...feature/x` please."),
            vec!["main...feature/x"]
        );
        assert_eq!(
            named_ranges("the range abc1234..def5678, then HEAD~3..HEAD."),
            vec!["abc1234..def5678", "HEAD~3..HEAD"]
        );
        assert_eq!(named_ranges("since v0.2.4..v0.2.5"), vec!["v0.2.4..v0.2.5"]);
        assert_eq!(
            named_ranges("compare release/a..release/b"),
            vec!["release/a..release/b"]
        );
        // prose, code and paths are not ranges
        for t in [
            "for i in 0..n { }",
            "steps 1..3",
            "let r = start..end;",
            "see ../notes/brief.md and a/../b",
            "and so on... then more",
            "wait...what",
        ] {
            assert!(named_ranges(t).is_empty(), "{t}: {:?}", named_ranges(t));
        }
        // duplicates are named once
        assert_eq!(named_ranges("main..dev and main..dev"), vec!["main..dev"]);
    }

    #[test]
    fn the_range_warning_applies_to_an_engine_without_a_shell_and_without_diff() {
        let w = range_without_shell_warning("grok", "", &["Review git diff main...topic"]).unwrap();
        assert!(
            w.starts_with(
                "the brief names the git range `main...topic`, but the grok reviewer has no shell"
            ),
            "{w}"
        );
        assert!(w.contains("pass -Diff main...topic"), "{w}");
        assert!(range_without_shell_warning("grok", "main...topic", &["main...topic"]).is_none());
        assert!(range_without_shell_warning("codex", "", &["main...topic"]).is_none());
        assert!(range_without_shell_warning("claude", "", &["nothing here"]).is_none());
        assert!(range_without_shell_warning("claude", "", &["", "HEAD~1..HEAD"]).is_some());
    }

    #[test]
    fn the_patch_takes_the_brief_stem_next_to_a_conventional_brief() {
        assert_eq!(
            patch_stem(Some("03-claude-acceptance.md"), true, 7, "claude"),
            "03-claude-acceptance"
        );
        assert_eq!(
            patch_stem(Some("12-lead-2-review-x.md"), true, 7, "lead-2"),
            "12-lead-2-review-x"
        );
        // not in this task's handoffs, not conventional, or named like a reply: the run's own name
        assert_eq!(
            patch_stem(Some("03-claude-acceptance.md"), false, 7, "claude"),
            "07-claude-diff"
        );
        assert_eq!(
            patch_stem(Some("brief.md"), true, 7, "claude"),
            "07-claude-diff"
        );
        assert_eq!(
            patch_stem(Some("04-codex-reply.md"), true, 12, "lead"),
            "12-lead-diff"
        );
        assert_eq!(patch_stem(None, false, 1, "claude"), "01-claude-diff");
    }

    #[test]
    fn diff_specs_parse_and_refuse() {
        assert_eq!(
            parse_diff_spec("main...topic").unwrap(),
            DiffSpec::Range {
                from: "main".into(),
                sep: "...",
                to: "topic".into()
            }
        );
        assert_eq!(
            parse_diff_spec("a..b").unwrap(),
            DiffSpec::Range {
                from: "a".into(),
                sep: "..",
                to: "b".into()
            }
        );
        assert_eq!(
            parse_diff_spec("HEAD").unwrap(),
            DiffSpec::WorkingTree { rev: "HEAD".into() }
        );
        for bad in ["", "a..", "..b", "a b", "-x", "a..-b", "a....b"] {
            assert!(parse_diff_spec(bad).is_err(), "{bad}");
        }
        assert!(rev_token_ok("v1.2"));
        assert!(!rev_token_ok("a..b"));
        assert!(!rev_token_ok("--all"));
    }

    // (B1) a range patch: the change of the range, `.collab/` and `.eck/` left out; one revision: the
    // working tree with a NEW (untracked) file included, the real index untouched.
    #[test]
    fn a_patch_holds_the_change_and_new_files_and_leaves_collab_out() {
        let r = repo("patch");
        let base = git_capture(&r, &["rev-parse", "HEAD"]).unwrap();
        std::fs::write(r.join("app.txt"), "two\n").unwrap();
        std::fs::write(r.join("added.txt"), "added\n").unwrap();
        std::fs::create_dir_all(r.join(".eck")).unwrap();
        std::fs::write(r.join(".eck").join("x.md"), "eck\n").unwrap();
        std::fs::create_dir_all(r.join(".collab/t/handoffs")).unwrap();
        std::fs::write(r.join(".collab/t/handoffs/01-claude-x.md"), "b\n").unwrap();
        git_capture(&r, &["add", "-A"]).unwrap();
        git_capture(&r, &["commit", "-q", "-m", "second"]).unwrap();
        let head = git_capture(&r, &["rev-parse", "HEAD"]).unwrap();
        let ex = diff_excludes(&r, &r.join(".collab"));
        let spec = check_diff_spec(&r, &format!("{}..{}", &base[..8], &head[..8])).unwrap();
        let p = String::from_utf8(render_patch(&r, &spec, &ex).unwrap()).unwrap();
        assert!(p.contains("diff --git a/app.txt b/app.txt"), "{p}");
        assert!(p.contains("+added"), "the added file is in the patch: {p}");
        assert!(!p.contains(".collab") && !p.contains(".eck"), "{p}");
        assert!(check_diff_spec(&r, "nope..HEAD").is_err());

        // one revision against the working tree: a modified file and an untracked new one
        std::fs::write(r.join("app.txt"), "three\n").unwrap();
        std::fs::write(r.join("brand-new.txt"), "fresh\n").unwrap();
        let index_before = std::fs::read(r.join(".git").join("index")).unwrap();
        let wt = render_patch(&r, &parse_diff_spec("HEAD").unwrap(), &ex).unwrap();
        let wt = String::from_utf8(wt).unwrap();
        assert!(wt.contains("+three"), "{wt}");
        assert!(
            wt.contains("brand-new.txt") && wt.contains("+fresh"),
            "{wt}"
        );
        assert_eq!(
            std::fs::read(r.join(".git").join("index")).unwrap(),
            index_before,
            "the real index is untouched"
        );
        assert!(
            git_capture(&r, &["status", "--porcelain"])
                .unwrap()
                .contains("?? brand-new.txt"),
            "the new file is still untracked"
        );

        // written next to a conventional brief, named by its content; the ledger record carries
        // range and sha256
        let handoffs = r.join(".collab/t/handoffs");
        let brief = handoffs.join("01-claude-x.md");
        let mut plan = plan_diff(&r, &handoffs, Some(&brief), 4, "claude", "HEAD");
        // (0.2.5 R8) the new file only with --diff-untracked
        plan.include_untracked = true;
        assert_eq!(plan.rel, ".collab/t/handoffs/01-claude-x.<sha8>.patch");
        assert!(!plan.written());
        write_patch(&mut plan, &r, &r.join(".collab")).unwrap();
        let on_disk = std::fs::read(&plan.path).unwrap();
        assert_eq!(plan.sha256, c3_core::sha256_hex(&on_disk));
        let rel = format!(".collab/t/handoffs/01-claude-x.{}.patch", &plan.sha256[..8]);
        assert_eq!(plan.rel, rel);
        assert_eq!(plan.files, 2);
        assert_eq!(plan.redactions, 0);
        let v = ledger_diff_value(&plan);
        assert_eq!(v["range"], "HEAD");
        assert_eq!(v["patch"], rel.as_str());
        assert_eq!(v["sha256"], plan.sha256.as_str());
        assert_eq!(v["redactions"], 0);
        let line = diff_prompt_line(&plan);
        assert!(
            line.starts_with(&format!(
                "The change under review is the patch at `{rel}` (`git diff HEAD`"
            )),
            "{line}"
        );
        assert!(
            line.ends_with("The patch is for reading, not for applying."),
            "{line}"
        );
        // a second write of the same bytes reuses the file
        write_patch(&mut plan, &r, &r.join(".collab")).unwrap();
        assert_eq!(plan.rel, rel);
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
    }

    // (0.2.5, S1) C3's folder `.c3/` and the legacy `.eck/` are both left out of a patch - tracked
    // (a range) or untracked (the working tree); `.c3ignore` wins over `.eckignore` in the same
    // directory, and a directory with only `.eckignore` is still honoured.
    #[test]
    fn a_patch_leaves_out_the_c3_and_legacy_folders_and_honours_c3ignore() {
        let r = repo("statedirs");
        let base = git_capture(&r, &["rev-parse", "HEAD"]).unwrap();
        for dir in [".c3", ".eck"] {
            std::fs::create_dir_all(r.join(dir)).unwrap();
            std::fs::write(r.join(dir).join("CONTEXT.md"), "STATE-FOLDER-TEXT\n").unwrap();
        }
        std::fs::write(r.join(".c3ignore"), "c3-hidden.txt\n").unwrap();
        std::fs::write(r.join(".eckignore"), "eck-only.txt\n").unwrap();
        std::fs::create_dir_all(r.join("sub")).unwrap();
        std::fs::write(r.join("sub").join(".eckignore"), "deep.txt\n").unwrap();
        for f in ["c3-hidden.txt", "eck-only.txt", "sub/deep.txt", "kept.txt"] {
            std::fs::write(r.join(f), format!("BODY-OF-{f}\n")).unwrap();
        }
        git_capture(&r, &["add", "-A", "-f"]).unwrap();
        git_capture(&r, &["commit", "-q", "-m", "state"]).unwrap();
        let head = git_capture(&r, &["rev-parse", "HEAD"]).unwrap();
        let ex = diff_excludes(&r, &r.join(".collab"));
        let spec = check_diff_spec(&r, &format!("{}..{}", &base[..8], &head[..8])).unwrap();
        let p = String::from_utf8(render_patch(&r, &spec, &ex).unwrap()).unwrap();
        assert!(!p.contains("STATE-FOLDER-TEXT"), "{p}");
        assert!(!p.contains("BODY-OF-c3-hidden.txt"), "{p}");
        assert!(
            p.contains("BODY-OF-eck-only.txt"),
            ".eckignore beside a .c3ignore is not read: {p}"
        );
        assert!(!p.contains("BODY-OF-sub/deep.txt"), "{p}");
        assert!(p.contains("BODY-OF-kept.txt"), "{p}");

        // the working tree: untracked files under both folders stay out
        for dir in [".c3", ".eck"] {
            std::fs::write(r.join(dir).join("NEW.md"), "UNTRACKED-STATE\n").unwrap();
        }
        std::fs::write(r.join("fresh.txt"), "FRESH\n").unwrap();
        let wt = render_patch(&r, &parse_diff_spec("HEAD").unwrap(), &ex).unwrap();
        let wt = String::from_utf8(wt).unwrap();
        assert!(!wt.contains("UNTRACKED-STATE"), "{wt}");
        assert!(wt.contains("+FRESH"), "{wt}");
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
    }

    // (R5, F19-2 / F18-4 / F20-6, the reviewers' RC2 / RC3) a patch never carries a secret off the
    // machine: a secret-shaped value is redacted (the rest of the patch kept, the sha256 of the file
    // as written, the count in the ledger record and the prompt line); an untracked file is the
    // change and stays in, but one a `.eckignore` excludes does not, and neither does a TRACKED file
    // the repository's ignore rules match; a hostile revision never reaches git as an option.
    #[test]
    fn a_patch_is_redacted_and_leaves_out_what_the_ignore_rules_exclude() {
        let r = repo("redact");
        std::fs::write(r.join(".gitignore"), "tracked-ignored.txt\n").unwrap();
        std::fs::write(r.join(".eckignore"), "private/\n").unwrap();
        std::fs::write(r.join("tracked-ignored.txt"), "old\n").unwrap();
        git_capture(&r, &["add", "-A"]).unwrap();
        git_capture(&r, &["add", "-f", "tracked-ignored.txt"]).unwrap();
        git_capture(&r, &["commit", "-q", "-m", "rules"]).unwrap();
        let key = format!("sk-or-v1-{}", "0123456789abcdef".repeat(4));
        std::fs::write(
            r.join("app.txt"),
            format!("one\nOPENROUTER_KEY = \"{key}\"\n"),
        )
        .unwrap();
        std::fs::write(r.join("tracked-ignored.txt"), "IGNORED-CHANGE\n").unwrap();
        std::fs::write(r.join("notes.txt"), "a new note\n").unwrap();
        std::fs::create_dir_all(r.join("private")).unwrap();
        std::fs::write(r.join("private").join("secret.txt"), "NEVER-SHARE\n").unwrap();
        let handoffs = r.join(".collab/t/handoffs");
        let mut plan = plan_diff(&r, &handoffs, None, 3, "claude", "HEAD");
        plan.include_untracked = true;
        write_patch(&mut plan, &r, &r.join(".collab")).unwrap();
        let text = std::fs::read_to_string(&plan.path).unwrap();
        assert!(!text.contains(&key), "{text}");
        assert!(text.contains("[REDACTED:"), "{text}");
        assert!(
            text.contains("+OPENROUTER_KEY = "),
            "the rest is kept: {text}"
        );
        assert!(
            text.contains("notes.txt") && text.contains("+a new note"),
            "{text}"
        );
        assert!(
            !text.contains("NEVER-SHARE") && !text.contains("private/secret.txt"),
            "{text}"
        );
        assert!(
            !text.contains("IGNORED-CHANGE") && !text.contains("tracked-ignored.txt"),
            "{text}"
        );
        assert!(plan.redactions >= 1, "{}", plan.redactions);
        assert_eq!(
            plan.sha256,
            c3_core::sha256_hex(&std::fs::read(&plan.path).unwrap())
        );
        assert_eq!(ledger_diff_value(&plan)["redactions"], plan.redactions);
        let line = diff_prompt_line(&plan);
        assert!(
            line.ends_with(&format!(
                "The patch is for reading, not for applying: {} secret-shaped value(s) in it were replaced by `[REDACTED:<kind>]` markers.",
                plan.redactions
            )),
            "{line}"
        );
        // a range form leaves the tracked ignored file out too
        git_capture(&r, &["add", "-A"]).unwrap();
        git_capture(&r, &["add", "-f", "tracked-ignored.txt"]).unwrap();
        git_capture(&r, &["commit", "-q", "-m", "change"]).unwrap();
        let ranged = String::from_utf8(
            render_patch(
                &r,
                &parse_diff_spec("HEAD~1..HEAD").unwrap(),
                &diff_excludes(&r, &r.join(".collab")),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(ranged.contains("+a new note"), "{ranged}");
        assert!(
            !ranged.contains("IGNORED-CHANGE") && !ranged.contains("NEVER-SHARE"),
            "{ranged}"
        );
        // (R5, F19-5) hostile values never become git options: refused before git, and git would
        // not read them as options anyway
        for bad in ["--output=x", "-c", "HEAD\nx", "a\u{1}b"] {
            assert!(parse_diff_spec(bad).is_err(), "{bad:?}");
            assert!(check_rev_value(bad).is_err(), "{bad:?}");
            assert!(worktree::resolve_commit(&r, bad).is_err(), "{bad:?}");
        }
        assert!(!r.join("x").exists(), "no git option was honoured");
        assert!(check_rev_value(&"a".repeat(REV_TOKEN_MAX + 1)).is_err());
        assert!(check_rev_value("main~2").is_ok());
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
    }

    // (R5, F19-3 / F20-3, the reviewers' RC3) one patch file per content: two ranges sharing one
    // brief stem get two files, each bound to its own bytes; the same range again reuses its file;
    // a file of that name holding other bytes is never overwritten (the write fails, the file keeps
    // its bytes).
    #[test]
    fn one_patch_file_per_content_and_none_is_ever_overwritten() {
        let r = repo("perfile");
        std::fs::write(r.join("app.txt"), "two\n").unwrap();
        git_capture(&r, &["commit", "-q", "-am", "second"]).unwrap();
        std::fs::write(r.join("app.txt"), "three\n").unwrap();
        let handoffs = r.join(".collab/t/handoffs");
        std::fs::create_dir_all(&handoffs).unwrap();
        let brief = handoffs.join("01-claude-x.md");
        std::fs::write(&brief, "b\n").unwrap();
        let collab = r.join(".collab");
        let mut a = plan_diff(&r, &handoffs, Some(&brief), 2, "claude", "HEAD~1..HEAD");
        let mut b = plan_diff(&r, &handoffs, Some(&brief), 3, "claude", "HEAD");
        write_patch(&mut a, &r, &collab).unwrap();
        write_patch(&mut b, &r, &collab).unwrap();
        assert_ne!(a.path, b.path, "two contents, two files");
        for p in [&a, &b] {
            assert_eq!(
                p.sha256,
                c3_core::sha256_hex(&std::fs::read(&p.path).unwrap()),
                "each record binds the bytes of its own file"
            );
        }
        // a sibling rendering the same range shares the file
        let mut a2 = plan_diff(&r, &handoffs, Some(&brief), 4, "claude", "HEAD~1..HEAD");
        write_patch(&mut a2, &r, &collab).unwrap();
        assert_eq!(a2.path, a.path);
        // another file at that name is never overwritten
        std::fs::remove_file(&a.path).unwrap();
        std::fs::write(&a.path, "somebody else's bytes\n").unwrap();
        let mut a3 = plan_diff(&r, &handoffs, Some(&brief), 5, "claude", "HEAD~1..HEAD");
        let err = write_patch(&mut a3, &r, &collab).unwrap_err();
        assert!(err.contains("never overwritten"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&a.path).unwrap(),
            "somebody else's bytes\n"
        );
        // no write temp is left behind
        assert!(!std::fs::read_dir(&handoffs)
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().ends_with(".tmp")));
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
    }

    // (B1) the plan: --rev for every local engine (refused when unknown), kimi always, claude on
    // diff-review / acceptance only, never http; the worktree stages the task files and watches
    // the reviewer's own writes.
    #[test]
    fn the_isolation_plan_and_the_staged_review_tree() {
        let r = repo("plan");
        let first = git_capture(&r, &["rev-parse", "HEAD"]).unwrap();
        std::fs::write(r.join("app.txt"), "two\n").unwrap();
        git_capture(&r, &["commit", "-q", "-am", "second"]).unwrap();
        let p = plan_isolation(&r, "grok", "framing", &first[..9], false)
            .unwrap()
            .unwrap();
        assert_eq!((p.reason, p.sha.as_str()), (Reason::Rev, first.as_str()));
        assert!(plan_isolation(&r, "codex", "", "no-such", false).is_err());
        // (R5, F19-1) the http engine reads --rev from a worktree too (its pack is built there);
        // without --rev it gets none
        let h = plan_isolation(&r, "http", "acceptance", &first, false)
            .unwrap()
            .unwrap();
        assert_eq!((h.reason, h.sha.as_str()), (Reason::Rev, first.as_str()));
        assert!(plan_isolation(&r, "http", "acceptance", "", false)
            .unwrap()
            .is_none());
        assert_eq!(
            plan_isolation(&r, "kimi", "framing", "", true)
                .unwrap()
                .unwrap()
                .reason,
            Reason::Engine
        );
        for (purpose, frozen) in [
            ("acceptance", true),
            ("diff-review", true),
            ("framing", false),
        ] {
            assert_eq!(
                plan_isolation(&r, "claude", purpose, "", false)
                    .unwrap()
                    .is_some(),
                frozen,
                "{purpose}"
            );
        }
        assert!(plan_isolation(&r, "codex", "acceptance", "", false)
            .unwrap()
            .is_none());

        // the staged tree: handoffs (no event streams), state.md and an uncommitted brief elsewhere
        let collab = r.join(".collab");
        let h = collab.join("t").join("handoffs");
        std::fs::create_dir_all(&h).unwrap();
        std::fs::write(h.join("01-claude-x.md"), "brief\n").unwrap();
        std::fs::write(h.join("02-codex-reply.events.jsonl"), "{}\n").unwrap();
        // (R5, F18-7) a bare `events.jsonl` is an event stream too
        std::fs::write(h.join("events.jsonl"), "{}\n").unwrap();
        std::fs::write(collab.join("t").join("state.md"), "state\n").unwrap();
        std::fs::create_dir_all(r.join("docs")).unwrap();
        std::fs::write(r.join("docs").join("brief.md"), "doc brief\n").unwrap();
        let tree = setup_review_tree(
            &p,
            &StageInputs {
                repo_root: &r,
                collab_root: &collab,
                task: "t",
                brief_ref: "docs/brief.md",
                brief_path: Some(&r.join("docs").join("brief.md")),
                engine: "grok",
            },
        )
        .unwrap();
        let wt = tree.path().to_path_buf();
        assert_eq!(tree.sha(), first);
        // (the checkout may carry CRLF under a global core.autocrlf)
        assert_eq!(
            std::fs::read_to_string(wt.join("app.txt"))
                .unwrap()
                .replace("\r\n", "\n"),
            "one\n"
        );
        assert!(wt.join(".collab/t/handoffs/01-claude-x.md").is_file());
        assert!(wt.join(".collab/t/state.md").is_file());
        assert!(!wt
            .join(".collab/t/handoffs/02-codex-reply.events.jsonl")
            .exists());
        assert!(!wt.join(".collab/t/handoffs/events.jsonl").exists());
        assert!(tree.handoffs_staged() && tree.links_skipped() == 0);
        assert!(wt.join("docs/brief.md").is_file());
        assert!(tree.changes().is_empty(), "{:?}", tree.changes());
        // (R5, F18-1) the records name the revision the worktree checks out
        assert_eq!(tree.reviewed_info().base_commit, first);
        assert!(first.starts_with(&tree.reviewed_info().short_sha));
        assert!(tree
            .isolation_line(&p)
            .contains(&format!("of revision {first} (--rev {};", &first[..9])));
        let v = ledger_worktree_value(&p, tree.sha(), Some(tree.path()));
        assert_eq!(v["reason"], "rev");
        assert_eq!(v["sha"], first.as_str());
        // (0.2.5 D7) the worktree's directory, for a run that resumes this run's thread
        assert_eq!(v["path"], wt.to_string_lossy().as_ref());
        assert!(ledger_worktree_value(&p, tree.sha(), None)
            .get("path")
            .is_none());

        // a change in the LIVE tree is not the worktree's; the reviewer's writes are
        std::fs::write(r.join("app.txt"), "live edit\n").unwrap();
        assert!(tree.changes().is_empty());
        std::fs::write(wt.join("app.txt"), "reviewer edit\n").unwrap();
        std::fs::write(wt.join(".collab/t/handoffs/99-mine.md"), "x\n").unwrap();
        assert_eq!(
            tree.changes(),
            vec![
                ".collab/t/handoffs/99-mine.md".to_string(),
                "app.txt".to_string()
            ]
        );
        drop(tree);
        assert!(std::fs::symlink_metadata(&wt).is_err(), "removed on drop");
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
    }

    /// A directory link at `link` to `target`: a junction on Windows (no privilege needed), a
    /// symlink elsewhere.
    fn dir_link(link: &Path, target: &Path) {
        #[cfg(windows)]
        {
            // (cmd's mklink takes `\` only)
            let win = |p: &Path| p.to_string_lossy().replace('/', "\\");
            let ok = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(win(link))
                .arg(win(target))
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            assert!(ok, "mklink /J failed");
        }
        #[cfg(not(windows))]
        std::os::unix::fs::symlink(target, link).unwrap();
    }

    fn stage(
        r: &Path,
        task: &str,
        brief_ref: &str,
        brief: Option<&Path>,
    ) -> Result<ReviewTree, SetupError> {
        let plan = plan_isolation(r, "kimi", "framing", "", true)
            .unwrap()
            .unwrap();
        setup_review_tree(
            &plan,
            &StageInputs {
                repo_root: r,
                collab_root: &r.join(".collab"),
                task,
                brief_ref,
                brief_path: brief,
                engine: "kimi",
            },
        )
    }

    // (R5, F18-5, the reviewer's RC4) staging never READS through a link: a `handoffs/` directory
    // that is a junction / symlink to an outside directory is not staged (counted), a link below
    // `handoffs/` is skipped (counted), and a brief reached through a link from outside the
    // repository refuses the run before anything starts - the outside sentinel never reaches the
    // worktree.
    #[test]
    fn staging_never_reads_through_a_link_and_nothing_outside_is_copied_in() {
        let r = repo("readlink");
        let outside = r.parent().unwrap().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("sentinel.txt"), "OUTSIDE-SECRET\n").unwrap();
        std::fs::write(outside.join("brief.md"), "# outside brief\n").unwrap();
        // task a: handoffs/ itself is a link to the outside directory
        std::fs::create_dir_all(r.join(".collab/a")).unwrap();
        dir_link(&r.join(".collab/a/handoffs"), &outside);
        let t = stage(&r, "a", "", None).unwrap();
        assert!(!t.handoffs_staged(), "a linked handoffs/ is not staged");
        assert_eq!(t.links_skipped(), 1);
        assert!(!t.path().join(".collab/a/handoffs").exists());
        drop(t);
        // task b: a real handoffs/ with a linked entry below it
        let hb = r.join(".collab/b/handoffs");
        std::fs::create_dir_all(&hb).unwrap();
        std::fs::write(hb.join("01-claude-x.md"), "brief\n").unwrap();
        dir_link(&hb.join("linked"), &outside);
        let t = stage(&r, "b", "", None).unwrap();
        assert!(t.handoffs_staged());
        assert_eq!(t.links_skipped(), 1);
        assert!(t.path().join(".collab/b/handoffs/01-claude-x.md").is_file());
        assert!(!t.path().join(".collab/b/handoffs/linked").exists());
        let found = walk_has(t.path(), "OUTSIDE-SECRET");
        assert!(!found, "outside bytes reached the worktree");
        drop(t);
        // a brief reached through a link from outside the repository refuses the run
        dir_link(&r.join("docs"), &outside);
        let bp = r.join("docs").join("brief.md");
        match stage(&r, "b", "docs/brief.md", Some(&bp)) {
            Err(SetupError::Refused(m)) => {
                assert!(m.contains("resolves outside the repository"), "{m}");
                assert!(crate::panel::run::is_preflight_refusal(&m), "{m}");
            }
            Err(e) => panic!("expected a typed staging refusal, got {e:?}"),
            Ok(_) => panic!("a brief read through a link was staged"),
        }
        let pre = worktree::isolated_brief_refusal(
            &r,
            "kimi",
            "docs/brief.md",
            Some(&bp),
            "HEAD",
            "HEAD",
        )
        .expect("the preflight refuses it too");
        assert!(pre.contains("resolves outside the repository"), "{pre}");
        assert_eq!(
            std::fs::read_to_string(outside.join("sentinel.txt")).unwrap(),
            "OUTSIDE-SECRET\n"
        );
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
    }

    /// Undo a race hook's swap of `dir` (the link removed, the aside directory renamed back).
    fn unswap(dir: &Path) {
        let mut aside_name = dir.file_name().unwrap().to_os_string();
        aside_name.push(".c3-race-orig");
        let aside = dir.with_file_name(aside_name);
        if aside.exists() {
            let _ = std::fs::remove_dir(dir).or_else(|_| std::fs::remove_file(dir));
            std::fs::rename(&aside, dir).unwrap();
        }
    }

    // (0.2.5 D4, F29-2 / F18-5) THE READ RACE: a staging source directory is swapped for a link to
    // an outside directory holding files of the same names. (a) After the brief's directory was
    // opened (`read-open`): on Windows the swap is refused (the handle is held); elsewhere it
    // happens and the brief is read from the ORIGINAL directory - the staged brief holds the live
    // bytes. (b) Before the walk reaches it (`read-walk`): the walk meets the link and the run is
    // refused (preflight-class); nothing outside is read. (c) `handoffs/` swapped after it was
    // opened, before a file is read (`stage-read`): the inside bytes are staged, never the outside
    // ones. In every case no outside byte reaches the worktree.
    #[test]
    fn a_staging_source_swapped_for_a_link_is_never_read_through() {
        let r = repo("race-read");
        let outside = r.parent().unwrap().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("brief.md"), "OUTSIDE-BRIEF\n").unwrap();
        std::fs::write(outside.join("01-claude-x.md"), "OUTSIDE-HANDOFF\n").unwrap();
        let docs = r.join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("brief.md"), "INSIDE-BRIEF\n").unwrap();
        let plan = plan_isolation(&r, "kimi", "framing", "", true)
            .unwrap()
            .unwrap();
        let no_collab = r.parent().unwrap().join("collab");
        let bp = docs.join("brief.md");
        let run = |collab: &Path, brief: &str| {
            setup_review_tree(
                &plan,
                &StageInputs {
                    repo_root: &r,
                    collab_root: collab,
                    task: "t",
                    brief_ref: brief,
                    brief_path: (!brief.is_empty()).then_some(bp.as_path()),
                    engine: "kimi",
                },
            )
        };
        let hook_on = |point: &str, dir: &Path| {
            c3_core::test_hooks::set_thread_hook(
                crate::contain::RACE_HOOK,
                &format!("{point}|{}|{}", dir.display(), outside.display()),
            )
        };
        // (a)
        let h = hook_on("read-open", &docs);
        let t = run(&no_collab, "docs/brief.md").unwrap_or_else(|e| panic!("(a) {e:?}"));
        drop(h);
        if cfg!(windows) {
            assert!(
                !r.join("docs.c3-race-orig").exists(),
                "(a) held: not renamed"
            );
        }
        assert_eq!(
            std::fs::read_to_string(t.path().join("docs").join("brief.md")).unwrap(),
            "INSIDE-BRIEF\n",
            "(a) read from the directory that was opened"
        );
        assert!(!walk_has(t.path(), "OUTSIDE"), "(a)");
        drop(t);
        unswap(&docs);
        // (b)
        let h = hook_on("read-walk", &docs);
        let got = run(&no_collab, "docs/brief.md");
        drop(h);
        let swapped = r.join("docs.c3-race-orig").exists();
        match got {
            Err(SetupError::Refused(m)) if swapped => {
                assert!(m.contains("resolves outside the repository"), "(b) {m}");
                assert!(crate::panel::run::is_preflight_refusal(&m), "(b) {m}");
            }
            Ok(t) if !swapped => {
                assert!(!walk_has(t.path(), "OUTSIDE"), "(b) not swapped");
            }
            Err(e) => panic!("(b) swapped={swapped}: {e:?}"),
            Ok(t) => panic!(
                "(b) a brief behind a swapped-in link was staged: {:?}",
                std::fs::read_to_string(t.path().join("docs").join("brief.md"))
            ),
        }
        unswap(&docs);
        // (c)
        let h_dir = r.join(".collab").join("t").join("handoffs");
        std::fs::create_dir_all(&h_dir).unwrap();
        std::fs::write(h_dir.join("01-claude-x.md"), "INSIDE-HANDOFF\n").unwrap();
        let h = hook_on("stage-read", &h_dir);
        let t = run(&r.join(".collab"), "").unwrap_or_else(|e| panic!("(c) {e:?}"));
        drop(h);
        if cfg!(windows) {
            assert!(
                !r.join(".collab/t/handoffs.c3-race-orig").exists(),
                "(c) held"
            );
        }
        assert_eq!(
            std::fs::read_to_string(t.path().join(".collab/t/handoffs/01-claude-x.md")).unwrap(),
            "INSIDE-HANDOFF\n"
        );
        assert!(!walk_has(t.path(), "OUTSIDE"), "(c)");
        drop(t);
        unswap(&h_dir);
        assert_eq!(
            std::fs::read_to_string(outside.join("brief.md")).unwrap(),
            "OUTSIDE-BRIEF\n"
        );
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
    }

    // (0.2.5 D4, F28-1) A patch on a file system without hard links (the test hook) is still put in
    // place from a COMPLETE temporary file: a write that fails half-way leaves neither the patch nor
    // a temporary file; the placement then puts the whole patch under its name; a sibling with the
    // same bytes reuses it; two placements racing on a fresh name both succeed with one whole file;
    // other bytes under the name are never overwritten. No temporary file is left at any point.
    #[test]
    fn a_patch_without_hard_links_is_placed_whole_and_nothing_partial_is_left() {
        let r = repo("nolink");
        std::fs::write(r.join("app.txt"), "two\n").unwrap();
        let handoffs = r.join(".collab/t/handoffs");
        std::fs::create_dir_all(&handoffs).unwrap();
        let collab = r.join(".collab");
        let names = |d: &Path| -> Vec<String> {
            let mut v: Vec<String> = std::fs::read_dir(d)
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            v.sort();
            v
        };
        let _nolink = c3_core::test_hooks::set_thread_hook(PATCH_NO_HARDLINK_HOOK, "1");
        {
            let _fail = c3_core::test_hooks::set_thread_hook(crate::contain::WRITE_FAIL_HOOK, "1");
            let mut p = plan_diff(&r, &handoffs, None, 2, "claude", "HEAD");
            let err = write_patch(&mut p, &r, &collab).unwrap_err();
            assert!(err.contains("injected write failure"), "{err}");
            assert!(names(&handoffs).is_empty(), "{:?}", names(&handoffs));
        }
        let mut p = plan_diff(&r, &handoffs, None, 2, "claude", "HEAD");
        write_patch(&mut p, &r, &collab).unwrap();
        let bytes = std::fs::read(&p.path).unwrap();
        assert_eq!(p.sha256, c3_core::sha256_hex(&bytes));
        let name = p.path.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(names(&handoffs), vec![name.clone()]);
        // (a panel member shares the panel's first handoff number, so the same stem)
        let mut p2 = plan_diff(&r, &handoffs, None, 2, "claude", "HEAD");
        write_patch(&mut p2, &r, &collab).unwrap();
        assert_eq!(p2.path, p.path, "a sibling reuses the file");
        // two placements racing on a fresh name
        std::fs::remove_file(&p.path).unwrap();
        let racers: Vec<_> = (0..2)
            .map(|_| {
                let (dir, rr, cc, name, bytes) = (
                    handoffs.clone(),
                    r.clone(),
                    collab.clone(),
                    name.clone(),
                    bytes.clone(),
                );
                std::thread::spawn(move || {
                    let _h = c3_core::test_hooks::set_thread_hook(PATCH_NO_HARDLINK_HOOK, "1");
                    place_patch(&dir, &rr, &cc, &name, &bytes)
                })
            })
            .collect();
        for t in racers {
            assert_eq!(t.join().unwrap(), Ok(()));
        }
        assert_eq!(std::fs::read(&p.path).unwrap(), bytes);
        assert_eq!(names(&handoffs), vec![name.clone()]);
        // other bytes under the name are never overwritten
        std::fs::remove_file(&p.path).unwrap();
        std::fs::write(&p.path, "somebody else's bytes\n").unwrap();
        let err = place_patch(&handoffs, &r, &collab, &name, &bytes).unwrap_err();
        assert!(err.contains("never overwritten"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&p.path).unwrap(),
            "somebody else's bytes\n"
        );
        assert_eq!(names(&handoffs), vec![name]);
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
    }

    /// Whether any regular file below `root` (links not followed) holds `needle`.
    fn walk_has(root: &Path, needle: &str) -> bool {
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let m = std::fs::symlink_metadata(e.path()).unwrap();
                if worktree::is_link(&m) {
                    continue;
                }
                if m.is_dir() {
                    stack.push(e.path());
                } else if std::fs::read(e.path())
                    .map(|b| String::from_utf8_lossy(&b).contains(needle))
                    .unwrap_or(false)
                {
                    return true;
                }
            }
        }
        false
    }

    // (R5, F20-4, the reviewer's RC2) the worktree check sees a write into a git-ignored path: the
    // before / after comparison includes ignored files; (F18-7) without a `handoffs/` directory
    // nothing claims the handoffs were copied.
    #[test]
    fn the_worktree_check_sees_a_write_to_an_ignored_path() {
        let r = repo("ignored");
        std::fs::write(r.join(".gitignore"), "build/\nignored.txt\n").unwrap();
        git_capture(&r, &["add", "-A"]).unwrap();
        git_capture(&r, &["commit", "-q", "-m", "ignore"]).unwrap();
        let t = stage(&r, "t", "", None).unwrap();
        assert!(!t.handoffs_staged(), "no handoffs/ directory, none staged");
        assert!(t.changes().is_empty(), "{:?}", t.changes());
        std::fs::create_dir_all(t.path().join("build")).unwrap();
        std::fs::write(t.path().join("build").join("out.txt"), "x\n").unwrap();
        std::fs::write(t.path().join("ignored.txt"), "y\n").unwrap();
        assert_eq!(
            t.changes(),
            vec!["build/out.txt".to_string(), "ignored.txt".to_string()]
        );
        // the records' fingerprint still leaves ignored files out
        assert_eq!(t.reviewed_info().changed_files, 0);
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
    }

    #[test]
    fn a_resume_on_another_revision_is_warned() {
        let mut e = c3_core::ledger::LedgerEntry {
            n: 3,
            base_commit: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            ..Default::default()
        };
        let other = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let w = resume_revision_warning("resume", "t-1", Some(&e), other, "topic").unwrap();
        assert!(
            w.contains("was recorded on revision aaaaaaaaaaaa (consult n=3)"),
            "{w}"
        );
        assert!(w.contains("this run reviews bbbbbbbbbbbb"), "{w}");
        // the same revision, a new thread, or no parent: nothing
        assert!(
            resume_revision_warning("fork", "t-1", Some(&e), &e.base_commit.clone(), "x").is_none()
        );
        assert!(resume_revision_warning("new", "", Some(&e), other, "x").is_none());
        // a parent that itself ran under --rev is compared by its worktree sha
        e.extra.insert(
            "worktree".into(),
            serde_json::json!({"reason": "rev", "rev": "topic", "sha": other}),
        );
        assert!(resume_revision_warning("fork", "t-1", Some(&e), other, "topic").is_none());
    }

    /// A ledger entry whose `worktree` record names `path` (none: no `path` key).
    fn entry_with_worktree(path: Option<&str>) -> c3_core::ledger::LedgerEntry {
        let mut e = c3_core::ledger::LedgerEntry {
            n: 1,
            thread: "session_1".into(),
            ..Default::default()
        };
        let mut w = serde_json::json!({"reason": "engine", "rev": "HEAD", "sha": "a".repeat(40)});
        if let Some(p) = path {
            w["path"] = serde_json::Value::String(p.to_string());
        }
        e.extra.insert("worktree".into(), w);
        e
    }

    fn engine_plan() -> IsolationPlan {
        IsolationPlan {
            reason: Reason::Engine,
            rev: "HEAD".into(),
            sha: "a".repeat(40),
            at: None,
            resume_note: String::new(),
        }
    }

    // (0.2.5 D7) A resume of a worktree-isolated engine's thread takes the parent run's worktree
    // path when it has C3's shape; a path of another shape gives a fresh path, a run warning and the
    // kimi failure's sentence; a parent without a recorded path a fresh path and the sentence only.
    #[test]
    fn a_resume_takes_the_parent_worktree_path_only_when_it_has_c3s_shape() {
        let own = std::env::temp_dir().join(format!(
            "{}{}",
            worktree::THROWAWAY_WORKTREE_PREFIX,
            uuid::Uuid::new_v4().simple()
        ));
        let own_s = own.to_string_lossy().to_string();
        let mut p = engine_plan();
        let parent = entry_with_worktree(Some(&own_s));
        assert_eq!(
            plan_resume_worktree(&mut p, "session_1", Some(&parent)),
            None
        );
        assert_eq!(p.at.as_deref(), Some(own.as_path()));
        assert!(p.resume_note.is_empty(), "{}", p.resume_note);

        // another shape (here: not under the system temp directory) is not used
        let base = repo("resume-shape");
        let elsewhere = base
            .join(format!(
                "{}{}",
                worktree::THROWAWAY_WORKTREE_PREFIX,
                uuid::Uuid::new_v4().simple()
            ))
            .to_string_lossy()
            .to_string();
        let mut p = engine_plan();
        let parent = entry_with_worktree(Some(&elsewhere));
        let w = plan_resume_worktree(&mut p, "session_1", Some(&parent))
            .expect("a run warning for a path that cannot be reused");
        assert!(
            w.contains(&format!(
                "the parent's worktree path {elsewhere} could not be reused"
            )),
            "{w}"
        );
        assert!(
            w.contains("not directly under the system temp directory"),
            "{w}"
        );
        assert!(p.at.is_none());
        assert!(p.resume_note.contains("--mode new"), "{}", p.resume_note);

        // no recorded path (an older build, or a parent that did not run isolated): a fresh path,
        // no warning, and the sentence a kimi "different directory" failure adds
        for parent in [Some(entry_with_worktree(None)), None] {
            let mut p = engine_plan();
            assert_eq!(
                plan_resume_worktree(&mut p, "session_1", parent.as_ref()),
                None
            );
            assert!(p.at.is_none());
            assert!(
                p.resume_note
                    .contains("was started where its directory is not known")
                    && p.resume_note.contains("start a new thread (--mode new)"),
                "{}",
                p.resume_note
            );
        }
        let _ = std::fs::remove_dir_all(base.parent().unwrap());
    }

    // (0.2.5 D7) A resume whose worktree path is taken (a live run of that thread, a crashed run's
    // leftover) is refused before anything starts, in one preflight-class line naming the path and
    // what to do; what sits there is left untouched. Once the path is free, the worktree is made
    // exactly there, and removed again when the tree drops.
    #[test]
    fn a_taken_resume_path_refuses_the_seat_and_a_free_one_is_used() {
        let r = repo("resume-at");
        let at = std::env::temp_dir().join(format!(
            "{}{}",
            worktree::THROWAWAY_WORKTREE_PREFIX,
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&at).unwrap();
        std::fs::write(at.join("left.txt"), "a leftover\n").unwrap();
        let mut plan = plan_isolation(&r, "kimi", "framing", "", true)
            .unwrap()
            .unwrap();
        plan.at = Some(at.clone());
        let inputs = StageInputs {
            repo_root: &r,
            collab_root: &r.join(".collab"),
            task: "t",
            brief_ref: "",
            brief_path: None,
            engine: "kimi",
        };
        match setup_review_tree(&plan, &inputs) {
            Err(SetupError::Refused(m)) => {
                assert!(
                    m.contains(&format!(
                        "the worktree path {} of the thread it resumes is taken",
                        at.display()
                    )),
                    "{m}"
                );
                assert!(m.contains("`c3 doctor --fix`"), "{m}");
                assert!(m.contains("nothing was started"), "{m}");
                assert!(!m.contains('\n'), "one line: {m}");
                assert!(crate::panel::run::is_preflight_refusal(&m), "{m}");
            }
            Err(e) => panic!("expected the typed refusal, got {e:?}"),
            Ok(t) => panic!("a taken path was used: {}", t.path().display()),
        }
        assert_eq!(
            std::fs::read_to_string(at.join("left.txt")).unwrap(),
            "a leftover\n",
            "what sits at the path is untouched"
        );
        std::fs::remove_dir_all(&at).unwrap();
        // the path is free now: the worktree is made exactly there and removed on drop
        let tree = setup_review_tree(&plan, &inputs).unwrap();
        assert_eq!(tree.path(), at.as_path());
        assert!(tree.path().join("app.txt").is_file());
        drop(tree);
        assert!(
            std::fs::symlink_metadata(&at).is_err(),
            "removed after the run"
        );
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
    }

    // (0.2.5 D1, F27-3) The collab comparison exempts exactly the patch the run wrote and its
    // write-temp name - nothing else that shares the patch's stem (a reviewer's `<stem>.notes.md`
    // stays a change).
    #[test]
    fn the_runs_own_patch_is_the_only_exempt_file() {
        let sha = format!("{}{}", "ab12cd34", "0".repeat(56));
        let mut plan = DiffPlan {
            stem: "04-claude-diff".into(),
            dir: PathBuf::from("c/t/handoffs"),
            ..Default::default()
        };
        // nothing is exempt before the patch was written
        assert!(!is_run_patch_file(
            &plan,
            "t",
            "t/handoffs/04-claude-diff.ab12cd34.patch"
        ));
        plan.sha256 = sha;
        plan.path = PathBuf::from("c/t/handoffs/04-claude-diff.ab12cd34.patch");
        let uuid = uuid::Uuid::new_v4().simple().to_string();
        assert_eq!(
            patch_temp_name("04-claude-diff.ab12cd34.patch", &uuid),
            format!("04-claude-diff.ab12cd34.{uuid}.tmp")
        );
        for exempt in [
            "t/handoffs/04-claude-diff.ab12cd34.patch".to_string(),
            "T/Handoffs/04-CLAUDE-DIFF.AB12CD34.PATCH".to_string(),
            format!("t/handoffs/04-claude-diff.ab12cd34.{uuid}.tmp"),
        ] {
            assert!(is_run_patch_file(&plan, "t", &exempt), "{exempt}");
        }
        for watched in [
            "t/handoffs/04-claude-diff.notes.md",
            "t/handoffs/04-claude-diff.ab12cd34.notes.md",
            "t/handoffs/04-claude-diff.ffffffff.patch",
            "t/handoffs/04-claude-diff.ab12cd34.patch.bak",
            "t/handoffs/04-claude-diff.ab12cd34.not-a-uuid.tmp",
            "t/handoffs/04-claude-diff.ab12cd34.0123456789abcdef0123456789abcdeg.tmp",
            "t/04-claude-diff.ab12cd34.patch",
            "other/handoffs/04-claude-diff.ab12cd34.patch",
        ] {
            assert!(!is_run_patch_file(&plan, "t", watched), "{watched}");
        }
    }

    // (0.2.5 D1, F28-2) More than 500 changed paths excluded by the ignore rules no longer refuse a
    // review: a range (`..` and `...`) and a one-revision diff leave every one of them out - a
    // modified, a deleted and a newly added excluded path included - and the patch of the rest is
    // byte for byte the plain `git diff` of it.
    #[test]
    fn many_excluded_paths_do_not_refuse_a_review() {
        let r = repo("hidden-many");
        std::fs::write(r.join(".gitignore"), "ign/\n").unwrap();
        std::fs::create_dir_all(r.join("ign")).unwrap();
        for i in 0..501 {
            std::fs::write(r.join("ign").join(format!("f{i}.txt")), "old\n").unwrap();
        }
        git_capture(&r, &["add", "-A"]).unwrap();
        git_capture(&r, &["add", "-f", "ign"]).unwrap();
        git_capture(&r, &["commit", "-q", "-m", "ignored files"]).unwrap();
        for i in 0..501 {
            std::fs::write(r.join("ign").join(format!("f{i}.txt")), "HIDDEN-CHANGE\n").unwrap();
        }
        std::fs::write(r.join("ign").join("added.txt"), "HIDDEN-ADDED\n").unwrap();
        std::fs::write(r.join("app.txt"), "two\n").unwrap();
        git_capture(&r, &["add", "-A"]).unwrap();
        git_capture(&r, &["add", "-f", "ign"]).unwrap();
        git_capture(&r, &["commit", "-q", "-m", "change"]).unwrap();
        let ex = diff_excludes(&r, &r.join(".collab"));
        let plain = git_capture(&r, &["diff", "--no-color", "HEAD~1..HEAD", "--", "app.txt"])
            .unwrap_or_default();
        for range in ["HEAD~1..HEAD", "HEAD~1...HEAD"] {
            let p = render_patch(&r, &parse_diff_spec(range).unwrap(), &ex)
                .unwrap_or_else(|e| panic!("{range}: {e}"));
            let p = String::from_utf8(p).unwrap();
            assert!(p.contains("+two"), "{range}: {p}");
            assert!(!p.contains("HIDDEN") && !p.contains("ign/"), "{range}: {p}");
            assert_eq!(p.trim_end(), plain.trim_end(), "{range}");
        }
        // the working tree: 501 modified excluded files, one deleted, one staged as new
        for i in 1..501 {
            std::fs::write(r.join("ign").join(format!("f{i}.txt")), "LIVE-HIDDEN\n").unwrap();
        }
        std::fs::remove_file(r.join("ign").join("f0.txt")).unwrap();
        std::fs::write(r.join("ign").join("staged.txt"), "STAGED-HIDDEN\n").unwrap();
        git_capture(&r, &["add", "-f", "ign/staged.txt"]).unwrap();
        std::fs::write(r.join("app.txt"), "three\n").unwrap();
        let p = render_patch(&r, &parse_diff_spec("HEAD").unwrap(), &ex).unwrap();
        let p = String::from_utf8(p).unwrap();
        assert!(p.contains("+three"), "{p}");
        assert!(!p.contains("HIDDEN") && !p.contains("ign/"), "{p}");
        // the real index is untouched: the staged file is still staged, nothing marked skip-worktree
        let staged = git_capture(&r, &["diff", "--cached", "--name-only"]).unwrap_or_default();
        assert_eq!(staged.trim(), "ign/staged.txt", "{staged}");
        let flags = git_capture(&r, &["ls-files", "-v", "ign/f1.txt"]).unwrap_or_default();
        assert!(flags.starts_with("H "), "{flags}");
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
    }

    // (0.2.5 D1, F28-3) A patch whose git output is not valid UTF-8 keeps the lossy conversion the
    // redactor needs, and says so: the plan counts the replaced sequences, the ledger record carries
    // `lossy_utf8: true` and the prompt line names the count; a valid patch says nothing.
    #[test]
    fn a_patch_that_is_not_utf8_is_said_to_be_altered() {
        for raw in [&b"plain"[..], b"a\xffb", b"caf\xe9 \xfe\xfd ok \xe2\x82"] {
            assert_eq!(
                invalid_utf8_sequences(raw),
                String::from_utf8_lossy(raw).matches('\u{FFFD}').count(),
                "{raw:?}"
            );
        }
        let r = repo("lossy");
        let handoffs = r.join(".collab/t/handoffs");
        std::fs::write(r.join("app.txt"), "valid change\n").unwrap();
        let mut ok = plan_diff(&r, &handoffs, None, 1, "claude", "HEAD");
        write_patch(&mut ok, &r, &r.join(".collab")).unwrap();
        assert_eq!(ok.lossy, 0);
        assert!(ledger_diff_value(&ok).get("lossy_utf8").is_none());
        assert!(!diff_prompt_line(&ok).contains("UTF-8"));
        std::fs::write(r.join("app.txt"), b"caf\xe9 au lait\n").unwrap();
        let mut plan = plan_diff(&r, &handoffs, None, 2, "claude", "HEAD");
        write_patch(&mut plan, &r, &r.join(".collab")).unwrap();
        assert_eq!(plan.lossy, 1, "{}", plan.rel);
        let v = ledger_diff_value(&plan);
        assert_eq!(v["lossy_utf8"], true, "{v}");
        assert_eq!(v["lossy_utf8_sequences"], 1, "{v}");
        let line = diff_prompt_line(&plan);
        assert!(
            line.ends_with(
                "1 byte sequence(s) in git's output were not valid UTF-8 and were replaced by U+FFFD, so the patch is not byte-exact there."
            ),
            "{line}"
        );
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
    }

    // (0.2.5 R8, F18-4 / F19-2 / F20-6, F53-5) A one-revision patch holds the tracked changes only:
    // the untracked files are left out and named - up to ten in the notice and the ledger, a name
    // with a backtick or a format character (U+202E) counted, never shown - and the prompt line says
    // so; `--diff-untracked` carries them and the notice says that instead.
    #[test]
    fn a_one_revision_patch_leaves_untracked_files_out_unless_asked() {
        let r = repo("untracked");
        let handoffs = r.join(".collab/t/handoffs");
        std::fs::write(r.join("app.txt"), "tracked change\n").unwrap();
        for i in 0..11 {
            std::fs::write(r.join(format!("new{i:02}.txt")), format!("NEW-BODY-{i}\n")).unwrap();
        }
        std::fs::write(r.join("odd`name.txt"), "ODD-BODY\n").unwrap();
        // (a format character in a harmless name: `<U+202E>txt.exe`, the name shape of a spoofed
        // executable, was quarantined by Windows Defender in the middle of this test)
        std::fs::write(r.join("a\u{202e}b.txt"), "BIDI-BODY\n").unwrap();
        // the default: tracked changes only, the untracked files named as left out
        let mut only = plan_diff(&r, &handoffs, None, 1, "claude", "HEAD");
        write_patch(&mut only, &r, &r.join(".collab")).unwrap();
        let text = std::fs::read_to_string(&only.path).unwrap();
        assert!(text.contains("+tracked change"), "{text}");
        assert!(
            !text.contains("NEW-BODY") && !text.contains("ODD-BODY") && !text.contains("BIDI"),
            "{text}"
        );
        assert!(only.untracked.is_empty());
        assert_eq!(
            only.untracked_left_out.len(),
            13,
            "{:?}",
            only.untracked_left_out
        );
        let notice = untracked_notice(&only).unwrap();
        assert!(
            notice.starts_with(
                "the -Diff patch holds the tracked changes only: 13 untracked file(s) were left out: `new00.txt`"
            ) && notice.contains("and 3 more")
                && notice.ends_with("pass --diff-untracked to include them"),
            "{notice}"
        );
        assert!(
            !notice.contains('\u{202e}') && !notice.contains("odd`"),
            "{notice}"
        );
        let v = ledger_diff_value(&only);
        assert_eq!(v["untracked_left_out_count"], 13, "{v}");
        assert_eq!(
            v["untracked_left_out"].as_array().map(|a| a.len()),
            Some(10),
            "{v}"
        );
        assert!(v.get("untracked").is_none(), "{v}");
        assert!(!v.to_string().contains('\u{202e}'), "{v}");
        assert!(diff_prompt_line(&only).contains(
            "13 untracked file(s) of the working tree were left out of it (only tracked changes are in the patch)."
        ));
        // --diff-untracked: carried, through the redactor, and the notice says so. (0.2.5 D5) The
        // the bidi-named file stays: where git can read it, it is in the patch like the rest; where
        // it cannot (a scanner took it away meanwhile) it is left out with its reason, counted,
        // never named - and the run goes on with the rest
        let bidi = "a\u{202e}b.txt";
        let mut plan = plan_diff(&r, &handoffs, None, 2, "claude", "HEAD");
        plan.include_untracked = true;
        write_patch(&mut plan, &r, &r.join(".collab")).unwrap();
        let text = std::fs::read_to_string(&plan.path).unwrap();
        assert!(
            text.contains("+NEW-BODY-10") && text.contains("+ODD-BODY"),
            "{text}"
        );
        let read = plan.untracked.iter().any(|p| p == bidi);
        let carried = if read { 13 } else { 12 };
        assert_eq!(plan.untracked.len(), carried, "{:?}", plan.untracked);
        if read {
            assert!(plan.untracked_left_out.is_empty());
            assert!(text.contains("+BIDI-BODY"), "{text}");
        } else {
            assert_eq!(plan.untracked_left_out, vec![bidi.to_string()]);
            assert!(!text.contains("BIDI-BODY"), "{text}");
        }
        let (named, more) = untracked_named(&plan);
        assert_eq!(named.len(), UNTRACKED_NAMED);
        assert_eq!(more, carried - UNTRACKED_NAMED);
        assert!(
            !named
                .iter()
                .any(|n| n.contains('`') || n.contains('\u{202e}')),
            "{named:?}"
        );
        let notice = untracked_notice(&plan).unwrap();
        assert!(
            notice.starts_with(&format!("the -Diff patch includes {carried} untracked file(s) with their content (--diff-untracked;")),
            "{notice}"
        );
        assert!(
            notice.contains("`new00.txt`")
                && notice.contains(&format!("and {} more", carried - UNTRACKED_NAMED)),
            "{notice}"
        );
        assert!(!notice.contains('\u{202e}'), "{notice}");
        let v = ledger_diff_value(&plan);
        assert_eq!(v["untracked_count"], carried, "{v}");
        assert_eq!(v["untracked"].as_array().map(|a| a.len()), Some(10), "{v}");
        assert!(!v.to_string().contains('\u{202e}'), "{v}");
        let line = diff_prompt_line(&plan);
        if read {
            assert!(v.get("untracked_left_out").is_none(), "{v}");
            assert!(!line.contains("left out of it"), "{line}");
        } else {
            assert!(
                notice.ends_with(
                    "; 1 untracked file(s) were left out - git could not read them: 1 name(s) not shown"
                ),
                "{notice}"
            );
            assert_eq!(v["untracked_left_out_count"], 1, "{v}");
            assert_eq!(v["untracked_left_out"], serde_json::json!([]), "{v}");
            assert_eq!(v["untracked_left_out_reason"], UNREADABLE_REASON, "{v}");
            assert!(
                line.contains(
                    " 1 untracked file(s) of the working tree were left out of it (git could not read them)."
                ),
                "{line}"
            );
        }
        // a range carries no untracked file and leaves none out
        git_capture(&r, &["add", "app.txt"]).unwrap();
        git_capture(&r, &["commit", "-q", "-m", "tracked"]).unwrap();
        let mut ranged = plan_diff(&r, &handoffs, None, 3, "claude", "HEAD~1..HEAD");
        write_patch(&mut ranged, &r, &r.join(".collab")).unwrap();
        assert!(ranged.untracked.is_empty() && ranged.untracked_left_out.is_empty());
        assert!(untracked_notice(&ranged).is_none());
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
    }

    // (0.2.5 D5) Under `--diff-untracked` an untracked file git cannot read (held locked by another
    // process on Windows, unreadable by its mode elsewhere) is left out with its reason - named,
    // since its name is plain - in the notice, the prompt line and the ledger, and the patch is made
    // with the files git could add. (A process that reads every file anyway - root - sees it carried.)
    #[test]
    fn an_untracked_file_git_cannot_read_is_left_out_and_the_run_goes_on() {
        let r = repo("unreadable");
        let handoffs = r.join(".collab/t/handoffs");
        std::fs::write(r.join("app.txt"), "tracked change\n").unwrap();
        std::fs::write(r.join("fresh.txt"), "FRESH-BODY\n").unwrap();
        let locked = r.join("locked.txt");
        std::fs::write(&locked, "LOCKED-BODY\n").unwrap();
        #[cfg(windows)]
        let hold = {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&locked)
                .unwrap()
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        let blocked = std::fs::read(&locked).is_err();
        let mut plan = plan_diff(&r, &handoffs, None, 1, "claude", "HEAD");
        plan.include_untracked = true;
        let written = write_patch(&mut plan, &r, &r.join(".collab"));
        #[cfg(windows)]
        drop(hold);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644));
        }
        written.unwrap();
        let text = std::fs::read_to_string(&plan.path).unwrap();
        assert!(
            text.contains("+tracked change") && text.contains("+FRESH-BODY"),
            "{text}"
        );
        if !blocked {
            assert_eq!(plan.untracked, vec!["fresh.txt", "locked.txt"]);
            assert!(plan.untracked_left_out.is_empty());
            let _ = std::fs::remove_dir_all(r.parent().unwrap());
            return;
        }
        assert!(!text.contains("LOCKED-BODY"), "{text}");
        assert_eq!(plan.untracked, vec!["fresh.txt"]);
        assert_eq!(plan.untracked_left_out, vec!["locked.txt"]);
        assert_eq!(
            untracked_notice(&plan).unwrap(),
            "the -Diff patch includes 1 untracked file(s) with their content (--diff-untracked; redacted only where a value looks like a secret): `fresh.txt`; 1 untracked file(s) were left out - git could not read them: `locked.txt`"
        );
        assert!(diff_prompt_line(&plan).ends_with(
            " 1 untracked file(s) of the working tree were left out of it (git could not read them)."
        ));
        let v = ledger_diff_value(&plan);
        assert_eq!(v["untracked"], serde_json::json!(["fresh.txt"]), "{v}");
        assert_eq!(v["untracked_count"], 1, "{v}");
        assert_eq!(
            v["untracked_left_out"],
            serde_json::json!(["locked.txt"]),
            "{v}"
        );
        assert_eq!(v["untracked_left_out_count"], 1, "{v}");
        assert_eq!(v["untracked_left_out_reason"], UNREADABLE_REASON, "{v}");
        // the only untracked file unreadable: the tracked changes alone, and the notice says why
        let mut alone = plan_diff(&r, &handoffs, None, 2, "claude", "HEAD");
        alone.include_untracked = true;
        std::fs::remove_file(r.join("fresh.txt")).unwrap();
        #[cfg(windows)]
        let hold = {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&locked)
                .unwrap()
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        let written = write_patch(&mut alone, &r, &r.join(".collab"));
        #[cfg(windows)]
        drop(hold);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644));
        }
        written.unwrap();
        assert!(alone.untracked.is_empty());
        assert_eq!(
            untracked_notice(&alone).unwrap(),
            "the -Diff patch holds the tracked changes only: 1 untracked file(s) were left out - git could not read them: `locked.txt`"
        );
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
    }

    /// The sorted names in `d` (nothing when it is not there).
    fn dir_names(d: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(d)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    // (0.2.5 D1, F43-1) The handoffs directory is reached through handles from the repository: a
    // link above `handoffs/` - the collab directory, or the task directory swapped for one while
    // the placement runs (the race hook) - refuses the patch, and nothing lands outside.
    #[test]
    fn a_link_above_the_handoffs_directory_refuses_the_patch() {
        let r = repo("patchdir-link");
        std::fs::write(r.join("app.txt"), "two\n").unwrap();
        let outside = r.parent().unwrap().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        // the collab directory is a link to a directory outside the repository
        crate::contain::tests::junction_or_symlink(&r.join(".collab"), &outside);
        let handoffs = r.join(".collab/t/handoffs");
        let mut p = plan_diff(&r, &handoffs, None, 2, "claude", "HEAD");
        let err = write_patch(&mut p, &r, &r.join(".collab")).unwrap_err();
        assert!(err.contains("is a link"), "{err}");
        assert!(dir_names(&outside).is_empty(), "{:?}", dir_names(&outside));
        crate::contain::remove_link(
            &cap_std::fs::Dir::open_ambient_dir(&r, cap_std::ambient_authority()).unwrap(),
            std::ffi::OsStr::new(".collab"),
            true,
        )
        .unwrap();
        // the task directory is swapped for a link after the anchor is open, before the walk
        std::fs::create_dir_all(r.join(".collab/t")).unwrap();
        let spec = format!(
            "patch-dir|{}|{}",
            r.join(".collab/t").display(),
            outside.display()
        );
        let _race = c3_core::test_hooks::set_thread_hook(crate::contain::RACE_HOOK, &spec);
        let mut p = plan_diff(&r, &handoffs, None, 2, "claude", "HEAD");
        let err = write_patch(&mut p, &r, &r.join(".collab")).unwrap_err();
        assert!(err.contains("is a link"), "{err}");
        assert!(dir_names(&outside).is_empty(), "{:?}", dir_names(&outside));
        drop(_race);
        // an ordinary collab directory: the directories are created through the handles
        let r2 = repo("patchdir-plain");
        std::fs::write(r2.join("app.txt"), "two\n").unwrap();
        let h2 = r2.join(".collab/t/handoffs");
        let mut p = plan_diff(&r2, &h2, None, 2, "claude", "HEAD");
        write_patch(&mut p, &r2, &r2.join(".collab")).unwrap();
        assert!(p.path.is_file());
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
        let _ = std::fs::remove_dir_all(r2.parent().unwrap());
    }

    // (0.2.5 R8, F51-3 / F52-2 / F53-6) No claims: an EMPTY file at the patch name (an earlier
    // build's claim) is replaced by the patch at once - with and without hard links -, nothing else is
    // left in the directory, and a file with other bytes is still never overwritten (nothing is
    // removed by name).
    #[test]
    fn an_empty_leftover_claim_is_replaced_and_other_bytes_are_refused() {
        let r = repo("empty-claim");
        std::fs::write(r.join("app.txt"), "two\n").unwrap();
        let handoffs = r.join(".collab/t/handoffs");
        let collab = r.join(".collab");
        let mut p = plan_diff(&r, &handoffs, None, 2, "claude", "HEAD");
        write_patch(&mut p, &r, &collab).unwrap();
        let bytes = std::fs::read(&p.path).unwrap();
        let name = p.path.file_name().unwrap().to_string_lossy().to_string();
        for nolink in [false, true] {
            std::fs::write(&p.path, b"").unwrap();
            let _h =
                nolink.then(|| c3_core::test_hooks::set_thread_hook(PATCH_NO_HARDLINK_HOOK, "1"));
            place_patch(&handoffs, &r, &collab, &name, &bytes).unwrap();
            assert_eq!(std::fs::read(&p.path).unwrap(), bytes, "nolink {nolink}");
            assert_eq!(dir_names(&handoffs), vec![name.clone()]);
        }
        for nolink in [false, true] {
            std::fs::write(&p.path, "somebody else's bytes\n").unwrap();
            let _h =
                nolink.then(|| c3_core::test_hooks::set_thread_hook(PATCH_NO_HARDLINK_HOOK, "1"));
            let err = place_patch(&handoffs, &r, &collab, &name, &bytes).unwrap_err();
            assert!(
                err.contains("holds other bytes") && err.contains("never overwritten"),
                "{err}"
            );
            assert_eq!(
                std::fs::read_to_string(&p.path).unwrap(),
                "somebody else's bytes\n"
            );
            assert_eq!(dir_names(&handoffs), vec![name.clone()]);
        }
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
    }

    // (0.2.5 R8, F53-4 / F43-7) A placement sweeps the write-temp files of C3's own pattern older
    // than a day from the handoffs directory; a fresh one (a sibling's placement in flight) and a
    // name of another pattern stay.
    #[test]
    fn stale_patch_temps_are_swept_when_a_patch_is_placed() {
        let r = repo("sweep");
        std::fs::write(r.join("app.txt"), "two\n").unwrap();
        let handoffs = r.join(".collab/t/handoffs");
        std::fs::create_dir_all(&handoffs).unwrap();
        let collab = r.join(".collab");
        let id = "0123456789abcdef0123456789abcdef";
        let stale = handoffs.join(format!("01-claude-diff.deadbeef.{id}.tmp"));
        let fresh = handoffs.join(format!("01-claude-diff.cafef00d.{id}.tmp"));
        let foreign = handoffs.join("notes.deadbeef.tmp");
        for f in [&stale, &fresh, &foreign] {
            std::fs::write(f, "partial").unwrap();
        }
        let old = std::time::SystemTime::now()
            - std::time::Duration::from_secs(PATCH_TEMP_STALE_SECS + 60);
        for f in [&stale, &foreign] {
            std::fs::File::options()
                .write(true)
                .open(f)
                .unwrap()
                .set_modified(old)
                .unwrap();
        }
        assert!(is_patch_temp_name(
            &stale.file_name().unwrap().to_string_lossy()
        ));
        assert!(!is_patch_temp_name("notes.deadbeef.tmp"));
        assert!(!is_patch_temp_name(&format!(".deadbeef.{id}.tmp")));
        let mut p = plan_diff(&r, &handoffs, None, 2, "claude", "HEAD");
        write_patch(&mut p, &r, &collab).unwrap();
        let names = dir_names(&handoffs);
        assert!(!stale.exists(), "{names:?}");
        assert!(fresh.exists() && foreign.exists(), "{names:?}");
        assert_eq!(names.len(), 3, "{names:?}");
        let _ = std::fs::remove_dir_all(r.parent().unwrap());
    }
}
