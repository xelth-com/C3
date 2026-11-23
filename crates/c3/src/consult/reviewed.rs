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
//!   `.eckignore`d files left out, secret-shaped values redacted) written into the task's handoffs
//!   as `<stem>.<sha8>.patch` - one file per content - next to the brief and named in the prompt.
//! * The warnings: a brief that names a git range given to an engine without a shell and without
//!   `--diff` ([`named_ranges`]); `--rev` resuming a thread recorded on another revision
//!   ([`resume_revision_warning`]).

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
        }));
    }
    if engine == "claude" && FROZEN_HEAD_PURPOSES.contains(&purpose) {
        return Ok(Some(IsolationPlan {
            reason: Reason::FrozenHead,
            rev: "HEAD".into(),
            sha: head(),
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

/// (R5, F18-5) What a staging source is, read WITHOUT following it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    Absent,
    /// A symlink / junction / reparse point, or a path whose canonical form leaves the repository
    /// (a link on its way): never read.
    Linked,
    Dir,
    File,
    Other,
}

fn source_kind(repo: &Path, p: &Path) -> SourceKind {
    let Ok(meta) = std::fs::symlink_metadata(p) else {
        return SourceKind::Absent;
    };
    if worktree::is_link(&meta) {
        return SourceKind::Linked;
    }
    let inside = std::fs::canonicalize(p)
        .ok()
        .zip(std::fs::canonicalize(repo).ok())
        .is_some_and(|(c, r)| c.starts_with(r));
    if !inside {
        return SourceKind::Linked;
    }
    if meta.is_dir() {
        SourceKind::Dir
    } else if meta.is_file() {
        SourceKind::File
    } else {
        SourceKind::Other
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
pub fn setup_review_tree(plan: &IsolationPlan, s: &StageInputs) -> Result<ReviewTree, SetupError> {
    let wt = ThrowawayWorktree::create(s.repo_root, plan.checkout()).map_err(|e| {
        SetupError::Capability(format!(
            "the reviewer's throwaway worktree could not be created: {e}"
        ))
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
    if let Some(cr) = &collab_rel {
        let task_dir = s.collab_root.join(s.task);
        let handoffs = task_dir.join("handoffs");
        match source_kind(s.repo_root, &handoffs) {
            SourceKind::Dir => {
                let st = wt
                    .stage_dir(
                        &handoffs,
                        &format!("{cr}/{}/handoffs", s.task),
                        &skip_staging,
                    )
                    .map_err(|e| refused("the task's handoffs", e))?;
                links_skipped += st.links_skipped;
                handoffs_staged = st.files > 0;
            }
            SourceKind::Linked => links_skipped += 1,
            _ => {}
        }
        let state = task_dir.join("state.md");
        match source_kind(s.repo_root, &state) {
            SourceKind::File => wt
                .stage_file(&state, &format!("{cr}/{}/state.md", s.task))
                .map_err(|e| refused("the task's state.md", e))?,
            SourceKind::Linked => links_skipped += 1,
            _ => {}
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
        let bytes = worktree::brief_bytes(s.repo_root, s.brief_ref, s.brief_path, wt.sha())
            .map_err(|e| refused(&what, e))?;
        wt.stage_bytes(&bytes, s.brief_ref)
            .map_err(|e| refused(&what, e))?;
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

/// The ledger's `worktree` record of a run that read a throwaway worktree: `{reason, rev, sha}`.
pub fn ledger_worktree_value(plan: &IsolationPlan, sha: &str) -> serde_json::Value {
    serde_json::json!({
        "reason": plan.reason.token(),
        "rev": plan.rev,
        "sha": if sha.is_empty() { plan.sha.as_str() } else { sha },
    })
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

/// The paths a patch leaves out: `.collab`, `.eck` and the collab directory (when it lies in the
/// repository under another name).
pub fn diff_excludes(repo_root: &Path, collab_root: &Path) -> Vec<String> {
    let mut v = vec![".collab".to_string(), ".eck".to_string()];
    if let Some(r) = c3_core::paths::repo_relative(repo_root, collab_root) {
        if !r.is_empty() && !v.contains(&r) {
            v.push(r);
        }
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

/// The most changed paths an ignore rule may hide from one patch (each becomes an `:(exclude)`
/// pathspec on the command line).
const PATCH_HIDDEN_MAX: usize = 500;

/// (R5, F19-2) The paths of `paths` (repo-relative, `/`) the repository excludes from what may leave
/// the machine: matched by its ignore rules (`git check-ignore --no-index`: `.gitignore` files,
/// `.git/info/exclude` and the global excludes - a TRACKED file included) or by a `.eckignore` file
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
    // `.eckignore`: per directory, from the repository root down (a deeper file decides)
    let mut cache: HashMap<PathBuf, Option<ignore::gitignore::Gitignore>> = HashMap::new();
    for p in paths {
        if out.contains(p) {
            continue;
        }
        let parts: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
        let mut full = repo.to_path_buf();
        for part in &parts {
            full.push(part);
        }
        let mut dir = repo.to_path_buf();
        let mut hit = false;
        for (i, part) in parts.iter().enumerate() {
            let gi = cache.entry(dir.clone()).or_insert_with(|| {
                let f = dir.join(".eckignore");
                f.is_file().then(|| {
                    let mut b = ignore::gitignore::GitignoreBuilder::new(&dir);
                    let _ = b.add(&f);
                    b.build().ok()
                })?
            });
            if let Some(g) = gi {
                match g.matched_path_or_any_parents(&full, false) {
                    ignore::Match::Ignore(_) => hit = true,
                    ignore::Match::Whitelist(_) => hit = false,
                    ignore::Match::None => {}
                }
            }
            if i + 1 < parts.len() {
                dir.push(part);
            }
        }
        if hit {
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

/// The patch bytes of `spec` in `repo`, the `excludes` left out. A range is `git diff <range>`; one
/// revision is diffed against the working tree with the untracked (not ignored) files marked
/// intent-to-add in a TEMPORARY copy of the index, so new files are in the patch and the real index
/// is never touched. (R5, F19-2) A file the repository's ignore rules or a `.eckignore` exclude
/// ([`ignore_excluded`]) never enters the patch - an untracked one is not marked, a tracked one is
/// left out by an `:(exclude,literal)` pathspec. (R5, F19-5) The revision or range is passed after
/// `--end-of-options`, so git never reads it as an option.
pub fn render_patch(repo: &Path, spec: &DiffSpec, excludes: &[String]) -> Result<Vec<u8>, String> {
    let mut pathspec: Vec<String> = vec!["--".into(), ".".into()];
    pathspec.extend(excludes.iter().map(|e| format!(":(exclude){e}")));
    let args_of = |head: &[&str], what: &str, hidden: &[String]| -> Vec<String> {
        let mut v: Vec<String> = ["-c", "core.quotepath=false", "diff"]
            .iter()
            .chain(head.iter())
            .map(|s| s.to_string())
            .collect();
        v.push("--end-of-options".into());
        v.push(what.to_string());
        v.extend(pathspec.iter().cloned());
        v.extend(hidden.iter().map(|p| format!(":(exclude,literal){p}")));
        v
    };
    // the changed paths the repository excludes, then the patch without them
    let diff_without_ignored = |what: &str, index: Option<&Path>| -> Result<Vec<u8>, String> {
        let names = args_of(&["--name-only", "-z", "--no-ext-diff"], what, &[]);
        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let changed = nul_list(&git_out(repo, &refs, index, None)?);
        let mut hidden: Vec<String> = ignore_excluded(repo, &changed)?.into_iter().collect();
        hidden.sort();
        if hidden.len() > PATCH_HIDDEN_MAX {
            return Err(format!(
                "the range changes {} files the repository's ignore rules or .eckignore exclude (at most {PATCH_HIDDEN_MAX} can be left out of one patch); narrow the range",
                hidden.len()
            ));
        }
        let args = args_of(&["--no-color", "--no-ext-diff"], what, &hidden);
        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        git_out(repo, &refs, index, None)
    };
    match spec {
        DiffSpec::Range { from, sep, to } => {
            let range = format!("{from}{sep}{to}");
            diff_without_ignored(&range, None)
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
            let tmp = std::env::temp_dir()
                .join(format!("c3-diff-index-{}", uuid::Uuid::new_v4().simple()));
            if index.is_file() {
                std::fs::copy(&index, &tmp)
                    .map_err(|e| format!("the index could not be copied ({e})"))?;
            }
            let result = (|| -> Result<Vec<u8>, String> {
                let mut ls: Vec<String> = vec![
                    "ls-files".into(),
                    "--others".into(),
                    "--exclude-standard".into(),
                    "-z".into(),
                ];
                ls.extend(pathspec.iter().cloned());
                let ls_refs: Vec<&str> = ls.iter().map(|s| s.as_str()).collect();
                let untracked = nul_list(&git_out(repo, &ls_refs, Some(&tmp), None)?);
                // (R5, F19-2) an untracked file is the change - unless a `.eckignore` excludes it
                // (`--exclude-standard` already left the git-ignored ones out)
                let hidden = ignore_excluded(repo, &untracked)?;
                let mut marked: Vec<u8> = Vec::new();
                for p in untracked.iter().filter(|p| !hidden.contains(*p)) {
                    marked.extend_from_slice(p.as_bytes());
                    marked.push(0);
                }
                if !marked.is_empty() {
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
                diff_without_ignored(rev, Some(&tmp))
            })();
            let _ = std::fs::remove_file(&tmp);
            let _ = std::fs::remove_file(tmp.with_extension("lock"));
            result
        }
    }
}

/// (R5, F19-3) Put `bytes` at `path` without ever overwriting a file: an existing regular file with
/// the SAME bytes is reused (a panel sibling, or an earlier run, rendered the same patch); one with
/// other bytes, or anything that is not a regular file (a link, a directory), is an `Err`. The bytes
/// go into a new temp file beside it (exclusive create) which is then hard-linked to the name - an
/// atomic create that fails when the name exists, so a sibling never reads a half-written file; on
/// a file system without hard links the name itself is created exclusively.
fn place_patch(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let shown = path.display().to_string();
    let reuse = || -> Result<(), String> {
        let m = std::fs::symlink_metadata(path)
            .map_err(|e| format!("{shown} cannot be checked ({e})"))?;
        if worktree::is_link(&m) || !m.is_file() {
            return Err(format!(
                "{shown} exists and is not a regular file; it is never reused or overwritten"
            ));
        }
        match std::fs::read(path) {
            Ok(b) if b == bytes => Ok(()),
            Ok(_) => Err(format!(
                "{shown} already holds other bytes; a patch file is never overwritten"
            )),
            Err(e) => Err(format!("{shown} cannot be read ({e})")),
        }
    };
    if std::fs::symlink_metadata(path).is_ok() {
        return reuse();
    }
    let dir = path
        .parent()
        .ok_or_else(|| format!("{shown} has no directory"))?;
    let _ = std::fs::create_dir_all(dir);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let tmp = dir.join(format!(
        "{}.{}.tmp",
        name.trim_end_matches(".patch"),
        uuid::Uuid::new_v4().simple()
    ));
    let written = (|| -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("{shown} could not be written ({e})"));
    }
    let linked = std::fs::hard_link(&tmp, path);
    let _ = std::fs::remove_file(&tmp);
    match linked {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => reuse(),
        Err(_) => {
            // no hard links here: create the name itself exclusively
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
            {
                Ok(mut f) => f
                    .write_all(bytes)
                    .and_then(|_| f.sync_all())
                    .map_err(|e| format!("{shown} could not be written ({e})")),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => reuse(),
                Err(e) => Err(format!("{shown} could not be written ({e})")),
            }
        }
    }
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
    let raw = render_patch(repo_root, &spec, &diff_excludes(repo_root, collab_root))
        .map_err(|e| format!("the -Diff patch could not be made: {e}"))?;
    let (safe, redactions) = crate::pack::redact::redact(&String::from_utf8_lossy(&raw));
    let bytes = safe.into_bytes();
    let sha = c3_core::sha256_hex(&bytes);
    let path = plan.dir.join(format!("{}.{}.patch", plan.stem, &sha[..8]));
    place_patch(&path, &bytes).map_err(|e| format!("the -Diff patch could not be written: {e}"))?;
    plan.rel = shown_path(repo_root, &path);
    plan.path = path;
    plan.sha256 = sha;
    plan.redactions = redactions;
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
        "The change under review is the patch at `{}` (`git diff {}`; `.collab/`, `.eck/` and files the repository's ignore rules or `.eckignore` exclude left out): read it first and review that change. The patch is for reading, not for applying",
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
    l
}

/// The ledger's `diff` record: `{range, patch, sha256, files, bytes, redactions}` (the sha256 of the
/// file as written).
pub fn ledger_diff_value(plan: &DiffPlan) -> serde_json::Value {
    serde_json::json!({
        "range": plan.range,
        "patch": plan.rel,
        "sha256": plan.sha256,
        "files": plan.files,
        "bytes": plan.bytes,
        "redactions": plan.redactions,
    })
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
        let v = ledger_worktree_value(&p, tree.sha());
        assert_eq!(v["reason"], "rev");
        assert_eq!(v["sha"], first.as_str());

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
}
