//! (release-0.2.5 wave B1) The revision a reviewer reads.
//!
//! * Which run gets a throwaway worktree ([`plan_isolation`]): `c3 consult --rev <rev>` for every
//!   engine that runs in a local working directory (codex, agy, muse, claude, grok, kimi - not the
//!   http engine, which sends a pack); the kimi engine always (a worktree of HEAD: it has no
//!   tool-restriction flag); a claude-engine reviewer on a `diff-review` / `acceptance` purpose (a
//!   frozen worktree of HEAD, so a parallel session editing the tree cannot change what it reads).
//! * Making it and staging the brief, the task's `handoffs/` and `state.md` into it at their
//!   repo-relative paths ([`setup_review_tree`]), and - after the last turn - the reviewer's own
//!   writes in it ([`ReviewTree::changes`]): that is what the read-only check watches, while a
//!   change of the live tree during the run is at most a warning.
//! * `--diff <range>` ([`plan_diff`], [`write_patch`]): `git diff <range>` written into the task's
//!   handoffs as `<NN>-<prefix>-<slug>.patch` next to the brief and named in the prompt.
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

/// A `--rev` value is one revision token: not empty, no blanks, not starting with `-` (git would
/// read it as an option), no `..` (that is a range).
pub fn rev_token_ok(rev: &str) -> bool {
    let r = rev.trim();
    !r.is_empty() && !r.starts_with('-') && !r.chars().any(char::is_whitespace) && !r.contains("..")
}

/// `--rev` resolved to the full sha of its commit, or the refusal (nothing was started).
pub fn resolve_rev(repo: &Path, rev: &str) -> Result<String, String> {
    worktree::resolve_commit(repo, rev).map_err(|why| format!("-Rev {why}; nothing was started."))
}

/// Which throwaway worktree (if any) a run of `engine` on `purpose` reads: `--rev` (resolved; a
/// revision git does not know is refused), else the engine's own isolation (`spec_isolated`: kimi),
/// else a claude-engine diff-review / acceptance (frozen HEAD), else none. The http engine never
/// gets one (it sends a pack of the live tree; the caller warns about `--rev`).
pub fn plan_isolation(
    repo: &Path,
    engine: &str,
    purpose: &str,
    rev: &str,
    spec_isolated: bool,
) -> Result<Option<IsolationPlan>, String> {
    if engine == "http" {
        return Ok(None);
    }
    let head = || worktree::resolve_commit(repo, "HEAD").unwrap_or_default();
    if !rev.trim().is_empty() {
        return Ok(Some(IsolationPlan {
            reason: Reason::Rev,
            rev: rev.trim().to_string(),
            sha: resolve_rev(repo, rev)?,
        }));
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
    before: RevisionInfo,
    before_collab: HashMap<String, String>,
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
/// review material).
fn skip_staging(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.to_ascii_lowercase().ends_with(".events.jsonl"))
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
    if let Some(cr) = &collab_rel {
        let task_dir = s.collab_root.join(s.task);
        let handoffs = task_dir.join("handoffs");
        if handoffs.is_dir() {
            wt.stage_dir(
                &handoffs,
                &format!("{cr}/{}/handoffs", s.task),
                &skip_staging,
            )
            .map_err(|e| refused("the task's handoffs", e))?;
        }
        let state = task_dir.join("state.md");
        if state.is_file() {
            wt.stage_file(&state, &format!("{cr}/{}/state.md", s.task))
                .map_err(|e| refused("the task's state.md", e))?;
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
    let before = revision::revision_info(wt.path(), collab.as_deref());
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

    /// The handoff `Isolation:` line ([`ThrowawayWorktree::isolation_line`]).
    pub fn isolation_line(&self, plan: &IsolationPlan) -> String {
        self.wt
            .isolation_line(plan.is_rev().then_some(plan.rev.as_str()))
    }

    /// The worktree-relative paths whose content appeared, disappeared or changed since the
    /// staging (the tracked and untracked files of the checkout, and every file of the staged
    /// collab copy, gitignored or not), ordinal-sorted. Only the reviewer's process tree runs
    /// there, so any entry is the reviewer's own write.
    pub fn changes(&self) -> Vec<String> {
        let after = revision::revision_info(self.wt.path(), self.collab.as_deref());
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
            "-Diff '{spec}' must be a range base..head or base...head, or one revision (diffed against the working tree, new files included); no spaces, no side starting with '-'."
        )
    };
    if s.is_empty() || s.chars().any(char::is_whitespace) {
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
/// once written - its sha256, file count and size.
#[derive(Debug, Clone, Default)]
pub struct DiffPlan {
    pub range: String,
    pub path: PathBuf,
    /// The path the prompt names: repo-relative (`/`-separated) when the collab directory lies in
    /// the repository, else absolute.
    pub rel: String,
    pub sha256: String,
    pub files: i64,
    pub bytes: u64,
}

/// The patch's file name in the task's handoffs: next to a brief named `<NN>-<prefix>-<slug>.md` in
/// those handoffs, the brief's own stem (`<NN>-<prefix>-<slug>.patch`); otherwise
/// `<nn>-<prefix>-diff.patch` with this run's handoff number and the coordinator's brief prefix. A
/// brief named like a reply (`04-codex-reply.md`) is not taken as a stem.
pub fn patch_name(
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
                        return format!("{stem}.patch");
                    }
                }
            }
        }
    }
    format!("{nn:02}-{prefix}-diff.patch")
}

/// Plan the `--diff` patch (nothing is written): its name ([`patch_name`]) in `handoffs_dir`.
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
    let name = patch_name(
        brief_path
            .and_then(|b| b.file_name())
            .and_then(|n| n.to_str()),
        in_handoffs,
        nn,
        prefix,
    );
    let path = handoffs_dir.join(name);
    let rel = c3_core::paths::repo_relative(repo_root, &path)
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    DiffPlan {
        range: spec.trim().to_string(),
        path,
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
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C").arg(repo).args(args);
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

/// The patch bytes of `spec` in `repo`, the `excludes` left out. A range is `git diff <range>`; one
/// revision is diffed against the working tree with the untracked (not ignored) files marked
/// intent-to-add in a TEMPORARY copy of the index, so new files are in the patch and the real index
/// is never touched.
pub fn render_patch(repo: &Path, spec: &DiffSpec, excludes: &[String]) -> Result<Vec<u8>, String> {
    let mut pathspec: Vec<String> = vec!["--".into(), ".".into()];
    pathspec.extend(excludes.iter().map(|e| format!(":(exclude){e}")));
    let diff_args = |what: &str| -> Vec<String> {
        let mut v: Vec<String> = [
            "-c",
            "core.quotepath=false",
            "diff",
            "--no-color",
            "--no-ext-diff",
            what,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        v.extend(pathspec.iter().cloned());
        v
    };
    match spec {
        DiffSpec::Range { from, sep, to } => {
            let range = format!("{from}{sep}{to}");
            let args = diff_args(&range);
            let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            git_out(repo, &refs, None, None)
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
                let untracked = git_out(repo, &ls_refs, Some(&tmp), None)?;
                if !untracked.is_empty() {
                    git_out(
                        repo,
                        &[
                            "add",
                            "--intent-to-add",
                            "--pathspec-from-file=-",
                            "--pathspec-file-nul",
                        ],
                        Some(&tmp),
                        Some(&untracked),
                    )?;
                }
                let args = diff_args(rev);
                let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                git_out(repo, &refs, Some(&tmp), None)
            })();
            let _ = std::fs::remove_file(&tmp);
            let _ = std::fs::remove_file(tmp.with_extension("lock"));
            result
        }
    }
}

/// Write the `--diff` patch to `plan.path` (atomically; an identical file is left as it is, and a
/// write that loses a race with a panel sibling writing the same bytes is fine), filling in its
/// sha256, file count and size. `Err` is the capability failure the run records.
pub fn write_patch(
    plan: &mut DiffPlan,
    repo_root: &Path,
    collab_root: &Path,
) -> Result<(), String> {
    let spec = parse_diff_spec(&plan.range)?;
    let bytes = render_patch(repo_root, &spec, &diff_excludes(repo_root, collab_root))
        .map_err(|e| format!("the -Diff patch could not be made: {e}"))?;
    let sha = c3_core::sha256_hex(&bytes);
    let same = |p: &Path| {
        std::fs::read(p)
            .map(|b| c3_core::sha256_hex(&b) == sha)
            .unwrap_or(false)
    };
    if !same(&plan.path) {
        if let Some(d) = plan.path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Err(e) = c3_core::store::write_text_atomic(&plan.path, &bytes) {
            if !same(&plan.path) {
                return Err(format!(
                    "the -Diff patch could not be written to {} ({e})",
                    plan.path.display()
                ));
            }
        }
    }
    plan.sha256 = sha;
    plan.bytes = bytes.len() as u64;
    plan.files = String::from_utf8_lossy(&bytes)
        .lines()
        .filter(|l| l.starts_with("diff --git "))
        .count() as i64;
    Ok(())
}

/// The prompt line that names the patch.
pub fn diff_prompt_line(plan: &DiffPlan) -> String {
    format!(
        "The change under review is the patch at `{}` (`git diff {}`; `.collab/` and `.eck/` left out): read it first and review that change.",
        plan.rel, plan.range
    )
}

/// The ledger's `diff` record: `{range, patch, sha256, files, bytes}`.
pub fn ledger_diff_value(plan: &DiffPlan) -> serde_json::Value {
    serde_json::json!({
        "range": plan.range,
        "patch": plan.rel,
        "sha256": plan.sha256,
        "files": plan.files,
        "bytes": plan.bytes,
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
            patch_name(Some("03-claude-acceptance.md"), true, 7, "claude"),
            "03-claude-acceptance.patch"
        );
        assert_eq!(
            patch_name(Some("12-lead-2-review-x.md"), true, 7, "lead-2"),
            "12-lead-2-review-x.patch"
        );
        // not in this task's handoffs, not conventional, or named like a reply: the run's own name
        assert_eq!(
            patch_name(Some("03-claude-acceptance.md"), false, 7, "claude"),
            "07-claude-diff.patch"
        );
        assert_eq!(
            patch_name(Some("brief.md"), true, 7, "claude"),
            "07-claude-diff.patch"
        );
        assert_eq!(
            patch_name(Some("04-codex-reply.md"), true, 12, "lead"),
            "12-lead-diff.patch"
        );
        assert_eq!(patch_name(None, false, 1, "claude"), "01-claude-diff.patch");
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

        // written next to a conventional brief; the ledger record carries range and sha256
        let handoffs = r.join(".collab/t/handoffs");
        let brief = handoffs.join("01-claude-x.md");
        let mut plan = plan_diff(&r, &handoffs, Some(&brief), 4, "claude", "HEAD");
        assert_eq!(plan.rel, ".collab/t/handoffs/01-claude-x.patch");
        write_patch(&mut plan, &r, &r.join(".collab")).unwrap();
        let on_disk = std::fs::read(&plan.path).unwrap();
        assert_eq!(plan.sha256, c3_core::sha256_hex(&on_disk));
        assert_eq!(plan.files, 2);
        let v = ledger_diff_value(&plan);
        assert_eq!(v["range"], "HEAD");
        assert_eq!(v["patch"], ".collab/t/handoffs/01-claude-x.patch");
        assert_eq!(v["sha256"], plan.sha256.as_str());
        assert!(diff_prompt_line(&plan).starts_with(
            "The change under review is the patch at `.collab/t/handoffs/01-claude-x.patch` (`git diff HEAD`"
        ));
        // a second write of the same bytes leaves the file alone
        write_patch(&mut plan, &r, &r.join(".collab")).unwrap();
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
        assert!(plan_isolation(&r, "http", "acceptance", &first, false)
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
        assert!(wt.join("docs/brief.md").is_file());
        assert!(tree.changes().is_empty(), "{:?}", tree.changes());
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
