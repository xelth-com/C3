//! Branch and worktree state (`c3 branches`, and the checks in `c3 doctor`, the SessionStart hook
//! and `c3 finish`).
//!
//! A branch's state is ONE line stored as its git description in the repository's own config
//! (`git config --local branch.<name>.description`): `c3: <state> | <owner> | <YYYY-MM-DD> | <text>`,
//! where `<state>` is `work` (in work; the owner names the session or person), `waits` (finished,
//! waits for the operator; the text is the question put to him) or `parked` (a WIP put aside; the
//! text is the condition for merging it). The description is shared by every worktree of the
//! repository and disappears with `git branch -d`. A description is C3's only when its FIRST line
//! parses as such a full record ([`BranchRecord::parse`]); anything else - a line `c3: hello`
//! included - is somebody's own note: it is kept byte for byte (a recorded state is written as the
//! first line above it, and `--clear` removes only that line) and the branch counts as having no
//! recorded state. The value is read with NUL framing and never trimmed; a value C3 cannot read
//! exactly (git fails, the value is not UTF-8, the key holds several values), or one that changed
//! between C3's read and its write, is never written over.
//!
//! The default branch is found independently of the invoking checkout ([`default_branch`]):
//! `c3.defaultBranch` in the local config (`c3 branches --default <name>`) wins, then `origin/HEAD`,
//! `init.defaultBranch`, `main` and `master`, each only when that local branch exists. Nothing else
//! is guessed: without one, every branch without a recorded state is unknown and the report says how
//! to set it.
//!
//! A branch is **unknown** when it is not merged by ancestry into the default branch AND has no
//! recorded state; a failed ancestry query leaves it unknown, never merged. Everything here only
//! READS the repository - every git call runs with `GIT_OPTIONAL_LOCKS=0`, so `git status` never
//! refreshes or locks the index - except [`set_state`], [`clear_state`] and [`set_default_branch`],
//! which write one config value each. Nothing is ever merged; the only thing ever deleted is a stale
//! throwaway worktree of a kimi review, by `c3 doctor --fix` ([`remove_stale_worktrees`]).
//!
//! Cost: a scan is a fixed handful of git calls (independent of the number of branches), plus, in
//! the full listing only, two per unmerged branch (`git cherry` and `git rev-list --count`) for at
//! most [`PATCH_CHECK_CAP`] of them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

/// The prefix that starts C3's record line.
pub const RECORD_PREFIX: &str = "c3: ";

/// The local git config key that names the default branch (`c3 branches --default <name>`); it
/// wins over every other source.
pub const DEFAULT_BRANCH_KEY: &str = "c3.defaultBranch";

/// What the reports say when no default branch can be named.
pub const DEFAULT_UNKNOWN: &str =
    "default branch unknown; set it with `c3 branches --default <name>`";

/// How many unmerged branches the full listing checks with `git cherry` and `git rev-list --count`:
/// two git processes per branch, and `git cherry` computes the patch id of every commit on both
/// sides since the merge base, so a long-diverged branch costs a read of each of those diffs. The
/// branches without a recorded state are checked first; the rest are listed "patches not checked".
pub const PATCH_CHECK_CAP: usize = 50;

/// A throwaway worktree of a kimi review with no owner record counts as stale after this long.
pub const STALE_AFTER: Duration = Duration::from_secs(6 * 3600);

/// The file in a kimi throwaway worktree's git admin directory (`<repo>/.git/worktrees/<name>/`)
/// that names the process owning it: `<pid> <start time>`. It is outside the worktree's files (the
/// reviewer's tree stays the clean HEAD) and goes with the admin directory when git removes or
/// prunes the worktree.
pub const THROWAWAY_OWNER_FILE: &str = "c3-owner";

/// The three recorded states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchState {
    /// In work; the owner names the session or person.
    Work,
    /// Finished, waits for the operator; the text is the question put to him.
    Waits,
    /// A WIP put aside; the text is the condition for merging it.
    Parked,
}

impl BranchState {
    /// The word written in the record.
    pub fn as_str(self) -> &'static str {
        match self {
            BranchState::Work => "work",
            BranchState::Waits => "waits",
            BranchState::Parked => "parked",
        }
    }

    /// Parse the word of a record (exact, lower case).
    pub fn parse(s: &str) -> Option<BranchState> {
        match s.trim() {
            "work" => Some(BranchState::Work),
            "waits" => Some(BranchState::Waits),
            "parked" => Some(BranchState::Parked),
            _ => None,
        }
    }
}

/// One recorded branch state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRecord {
    pub state: BranchState,
    /// The owning session or person; may be empty.
    pub owner: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// The question (`waits`), the merge condition (`parked`) or a note (`work`).
    pub text: String,
}

/// Collapse line breaks to spaces (a record is one line).
fn one_line(s: &str) -> String {
    s.split(['\r', '\n'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

impl BranchRecord {
    /// The record line: `c3: <state> | <owner> | <date> | <text>`. Line breaks become spaces; a `|`
    /// in the owner or the date (fields that are split on it) becomes `/`. A `|` in the text is
    /// kept: the text is the last field.
    pub fn format(&self) -> String {
        let field = |s: &str| one_line(s).replace('|', "/");
        format!(
            "{RECORD_PREFIX}{} | {} | {} | {}",
            self.state.as_str(),
            field(&self.owner),
            field(&self.date),
            one_line(&self.text)
        )
    }

    /// Parse a description. Only its FIRST line is the record; `None` when the description is not
    /// C3's (no `c3: ` prefix) or the record is malformed (unknown state, missing fields).
    pub fn parse(description: &str) -> Option<BranchRecord> {
        let first = description.lines().next()?;
        let rest = first.strip_prefix(RECORD_PREFIX)?;
        let mut parts = rest.splitn(4, '|');
        let state = BranchState::parse(parts.next()?)?;
        let owner = parts.next()?.trim().to_string();
        let date = parts.next()?.trim().to_string();
        let text = parts.next()?.trim().to_string();
        Some(BranchRecord {
            state,
            owner,
            date,
            text,
        })
    }

    /// A short human rendering: `waits (owner, 2026-11-18): the question`.
    pub fn describe(&self) -> String {
        let who = if self.owner.is_empty() {
            self.date.clone()
        } else {
            format!("{}, {}", self.owner, self.date)
        };
        if self.text.is_empty() {
            format!("{} ({who})", self.state.as_str())
        } else {
            format!("{} ({who}): {}", self.state.as_str(), self.text)
        }
    }
}

/// Whether a description is C3's: its first line parses as a full record. A foreign line that
/// merely starts with `c3: ` is not.
pub fn is_c3_description(description: &str) -> bool {
    BranchRecord::parse(description).is_some()
}

/// The foreign part of a description, exactly as stored. For C3's description: `None` when it is the
/// record line alone, else everything after the record line's line break (possibly empty). For a
/// foreign description: all of it.
pub fn foreign_part(description: &str) -> Option<&str> {
    if !is_c3_description(description) {
        return Some(description);
    }
    description.split_once('\n').map(|(_, rest)| rest)
}

/// The new description for `--set`: the record line on top of the foreign part, which is kept byte
/// for byte (`existing` is `None` when the branch has no description).
pub fn description_with_record(existing: Option<&str>, record: &BranchRecord) -> String {
    match existing.and_then(foreign_part) {
        None => record.format(),
        Some(foreign) => format!("{}\n{foreign}", record.format()),
    }
}

// ---------------------------------------------------------------------------------------------
// git plumbing
// ---------------------------------------------------------------------------------------------

/// A git command in `repo` that never takes an optional lock (`GIT_OPTIONAL_LOCKS=0`: `git status`
/// does not refresh or lock the index; a config write still takes its own required lock).
fn git_cmd(repo: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).env("GIT_OPTIONAL_LOCKS", "0");
    cmd
}

/// Run git in `repo`; stdout on success, `None` on any failure (spawn error or non-zero exit).
fn git_out(repo: &Path, args: &[&str]) -> Option<String> {
    let out = git_cmd(repo).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Run git in `repo`; true when it exits 0.
fn git_ok(repo: &Path, args: &[&str]) -> bool {
    git_cmd(repo)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Whether a local branch of that name exists.
pub fn branch_exists(repo: &Path, name: &str) -> bool {
    if name.trim().is_empty() {
        return false;
    }
    git_ok(
        repo,
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{name}"),
        ],
    )
}

/// Refuse a name git would not accept as a branch name: empty, starting with `-`, holding a control
/// character, or failing `git check-ref-format refs/heads/<name>` (`..`, a space, `~^:?*[\`, `@{`, a
/// trailing `.lock`, `/` or `.`). A valid name with dots, slashes or non-ASCII characters is fine:
/// git splits a config key at its FIRST and LAST dot, so `branch.<name>.description` addresses that
/// branch's own subsection.
pub fn check_branch_name(repo: &Path, name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && !name.starts_with('-')
        && !name.chars().any(char::is_control)
        && git_ok(repo, &["check-ref-format", &format!("refs/heads/{name}")]);
    if valid {
        Ok(())
    } else {
        Err(format!(
            "'{}' is not a valid branch name",
            name.escape_debug()
        ))
    }
}

/// The repository's default branch, found independently of the checkout C3 runs in. In order, each
/// only when that local branch exists: `c3.defaultBranch` in the local config (it wins over
/// everything; naming a missing branch is an error, not a fallback), the branch `origin/HEAD` points
/// to, `init.defaultBranch`, `main`, `master`. `Err` (the reason, with how to set it) when none
/// applies: the invoking checkout's branch is never taken for the default.
pub fn default_branch(repo: &Path) -> Result<String, String> {
    if let Some(v) = git_out(repo, &["config", "--local", "--get", DEFAULT_BRANCH_KEY]) {
        let v = v.trim();
        if !v.is_empty() {
            return if branch_exists(repo, v) {
                Ok(v.to_string())
            } else {
                Err(format!(
                    "{DEFAULT_BRANCH_KEY} names '{v}', which is not a local branch; set it with `c3 branches --default <name>`"
                ))
            };
        }
    }
    if let Some(s) = git_out(
        repo,
        &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
    ) {
        if let Some(name) = s.trim().strip_prefix("refs/remotes/origin/") {
            if branch_exists(repo, name) {
                return Ok(name.to_string());
            }
        }
    }
    if let Some(v) = git_out(repo, &["config", "--get", "init.defaultBranch"]) {
        let v = v.trim();
        if branch_exists(repo, v) {
            return Ok(v.to_string());
        }
    }
    for name in ["main", "master"] {
        if branch_exists(repo, name) {
            return Ok(name.to_string());
        }
    }
    Err(DEFAULT_UNKNOWN.to_string())
}

/// `c3 branches --default <name>`: store the default branch in the local config
/// ([`DEFAULT_BRANCH_KEY`]). Refuses an invalid name and one that is not a local branch.
pub fn set_default_branch(repo: &Path, name: &str) -> Result<(), String> {
    check_branch_name(repo, name)?;
    if !branch_exists(repo, name) {
        return Err(format!("no local branch named '{name}'"));
    }
    write_config(repo, DEFAULT_BRANCH_KEY, name)
}

/// Parse `git config --null --get-regexp '^branch\..*\.description$'` output (`key\nvalue\0`...)
/// into branch name -> description.
pub fn parse_descriptions(out: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for entry in out.split('\0') {
        if entry.is_empty() {
            continue;
        }
        let (key, value) = entry.split_once('\n').unwrap_or((entry, ""));
        let Some(name) = key
            .strip_prefix("branch.")
            .and_then(|k| k.strip_suffix(".description"))
        else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        map.insert(name.to_string(), value.to_string());
    }
    map
}

/// Every branch description of the repository's own config (one `git config` call; empty on any
/// error).
fn read_descriptions(repo: &Path) -> BTreeMap<String, String> {
    git_out(
        repo,
        &[
            "config",
            "--local",
            "--null",
            "--get-regexp",
            r"^branch\..*\.description$",
        ],
    )
    .map(|o| parse_descriptions(&o))
    .unwrap_or_default()
}

/// One entry of `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorktreeEntry {
    pub path: String,
    pub head: String,
    /// The checked-out branch (short name), `None` when detached or bare.
    pub branch: Option<String>,
    pub bare: bool,
    pub prunable: bool,
}

/// Parse `git worktree list --porcelain`. The first entry is the primary worktree.
pub fn parse_worktrees(out: &str) -> Vec<WorktreeEntry> {
    let mut list = Vec::new();
    let mut cur: Option<WorktreeEntry> = None;
    for line in out.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(p) = line.strip_prefix("worktree ") {
            if let Some(w) = cur.take() {
                list.push(w);
            }
            cur = Some(WorktreeEntry {
                path: p.to_string(),
                ..Default::default()
            });
            continue;
        }
        let Some(w) = cur.as_mut() else { continue };
        if let Some(h) = line.strip_prefix("HEAD ") {
            w.head = h.to_string();
        } else if let Some(b) = line.strip_prefix("branch ") {
            w.branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
        } else if line == "bare" {
            w.bare = true;
        } else if line == "prunable" || line.starts_with("prunable ") {
            w.prunable = true;
        }
    }
    if let Some(w) = cur.take() {
        list.push(w);
    }
    list
}

// ---------------------------------------------------------------------------------------------
// The report
// ---------------------------------------------------------------------------------------------

/// One local branch (the default branch is never listed).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BranchInfo {
    pub name: String,
    /// C3's recorded state, when the description carries one.
    pub record: Option<BranchRecord>,
    /// The description holds somebody's own note (kept; not a recorded state).
    pub foreign_note: bool,
    /// Merged into the default branch by ancestry; `None` when that is not known (no default
    /// branch, or the ancestry query failed) - such a branch is never taken for merged.
    pub merged: Option<bool>,
    /// Not merged by ancestry, but `git cherry refs/heads/<default> refs/heads/<name>` prints no
    /// `+` line: every commit has a patch-equivalent commit on the default branch (the cherry-pick
    /// case). Historical patch equivalence, not a claim about the default branch's current content
    /// (a later revert is not seen). Only computed by a full scan, for at most [`PATCH_CHECK_CAP`]
    /// unmerged branches.
    pub patches_on_default: Option<bool>,
    /// The full scan ran the patch check on this branch.
    pub patches_checked: bool,
    /// Commits ahead of the default branch. Only computed by a full scan.
    pub ahead: Option<u64>,
    /// Date of the last commit (`YYYY-MM-DD`).
    pub last_commit: String,
    /// The worktree that has it checked out, if any.
    pub worktree: Option<String>,
}

impl BranchInfo {
    /// Not known to be merged by ancestry AND no recorded state.
    pub fn is_unknown(&self) -> bool {
        self.merged != Some(true) && self.record.is_none()
    }
}

/// A worktree other than the primary one and the one of the default branch.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorktreeInfo {
    pub path: String,
    /// `None`: detached HEAD.
    pub branch: Option<String>,
    /// The branch has a recorded state (always false for a detached worktree).
    pub recorded: bool,
    /// Git marks it prunable (its directory is gone).
    pub prunable: bool,
}

/// A throwaway worktree of a kimi review that no live run owns any more.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StaleWorktree {
    pub path: String,
    /// Why it counts as stale.
    pub why: String,
}

/// The start folder: the primary worktree.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StartFolder {
    pub path: String,
    /// `None`: detached HEAD.
    pub branch: Option<String>,
    /// On the default branch (false when no default branch is known).
    pub on_default: bool,
    /// Commits behind `behind_of`, as the local refs know it (no fetch).
    pub behind: Option<u64>,
    /// The default branch's upstream (`origin/main`), else the default branch itself; empty when no
    /// default branch is known.
    pub behind_of: String,
    /// Uncommitted changes (a count). Only computed when asked for.
    pub dirty: Option<usize>,
}

/// Everything a scan found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BranchReport {
    /// `None` when no default branch can be named ([`default_branch`]; `default_error` says why).
    pub default_branch: Option<String>,
    pub default_error: Option<String>,
    /// The ancestry query against the default branch failed: no branch counts as merged.
    pub ancestry_failed: bool,
    pub branches: Vec<BranchInfo>,
    pub worktrees: Vec<WorktreeInfo>,
    pub start: Option<StartFolder>,
    /// Unmerged branches the full listing did not patch-check ([`PATCH_CHECK_CAP`]).
    pub patches_unchecked: usize,
    /// Throwaway worktrees of kimi reviews that no live run owns ([`remove_stale_worktrees`]).
    pub stale_worktrees: Vec<StaleWorktree>,
}

impl BranchReport {
    /// The unknown branches, in name order.
    pub fn unknown_branches(&self) -> Vec<&BranchInfo> {
        self.branches.iter().filter(|b| b.is_unknown()).collect()
    }

    /// The worktrees whose branch has no recorded state (detached ones included).
    pub fn unrecorded_worktrees(&self) -> Vec<&WorktreeInfo> {
        self.worktrees.iter().filter(|w| !w.recorded).collect()
    }

    /// Nothing unknown: the default branch is known, every listed branch is merged or recorded,
    /// every worktree recorded. (Stale throwaway worktrees are counted separately.)
    pub fn is_clean(&self) -> bool {
        self.default_branch.is_some()
            && self.unknown_branches().is_empty()
            && self.unrecorded_worktrees().is_empty()
    }

    /// The default branch's name for messages (`the default branch` when it is unknown).
    pub fn default_label(&self) -> &str {
        self.default_branch
            .as_deref()
            .unwrap_or("the default branch")
    }

    /// Why no default branch is known (`None` when one is).
    pub fn default_unknown_text(&self) -> Option<&str> {
        if self.default_branch.is_some() {
            return None;
        }
        Some(self.default_error.as_deref().unwrap_or(DEFAULT_UNKNOWN))
    }
}

/// How much a scan computes.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScanOpts {
    /// Per unmerged branch (at most [`PATCH_CHECK_CAP`]): `git cherry` (patches on default) and the
    /// ahead count. The full listing only; the hook never does this.
    pub content: bool,
    /// The start folder's uncommitted-change count (`git status --porcelain`).
    pub dirty: bool,
}

/// Whether a branch is merged by ancestry, given the `--no-merged` set (`None`: the query failed
/// or there is no default branch - then it is not known, never merged).
pub fn merged_state(no_merged: Option<&BTreeSet<String>>, name: &str) -> Option<bool> {
    no_merged.map(|set| !set.contains(name))
}

/// Scan the repository at `repo`. `None` outside a git repository or in one without any local
/// branch (nothing to report); every other git error degrades to missing data (an unknown default
/// branch, an unknown ancestry), never to a failure.
pub fn scan(repo: &Path, opts: ScanOpts) -> Option<BranchReport> {
    // Every local branch with its last commit date (one call).
    let all = git_out(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname)%09%(committerdate:short)",
            "refs/heads",
        ],
    )?;
    let refs: Vec<(String, String)> = all
        .lines()
        .filter_map(|line| {
            let (refname, date) = line.split_once('\t').unwrap_or((line, ""));
            let name = refname.trim().strip_prefix("refs/heads/")?;
            Some((name.to_string(), date.trim().to_string()))
        })
        .collect();
    if refs.is_empty() {
        return None;
    }

    let (default, default_error) = match default_branch(repo) {
        Ok(d) => (Some(d), None),
        Err(e) => (None, Some(e)),
    };
    // The unmerged branches (one call); `None` when it failed or there is no default branch.
    let no_merged: Option<BTreeSet<String>> = default.as_deref().and_then(|d| {
        git_out(
            repo,
            &[
                "for-each-ref",
                &format!("--no-merged=refs/heads/{d}"),
                "--format=%(refname)",
                "refs/heads",
            ],
        )
        .map(|o| {
            o.lines()
                .filter_map(|l| l.trim().strip_prefix("refs/heads/").map(str::to_string))
                .collect()
        })
    });
    let ancestry_failed = default.is_some() && no_merged.is_none();
    let is_default = |name: &str| default.as_deref() == Some(name);

    let descriptions = read_descriptions(repo);
    let worktrees = git_out(repo, &["worktree", "list", "--porcelain"])
        .map(|o| parse_worktrees(&o))
        .unwrap_or_default();

    let mut branch_worktree: BTreeMap<String, String> = BTreeMap::new();
    for w in &worktrees {
        if let Some(b) = &w.branch {
            branch_worktree
                .entry(b.clone())
                .or_insert_with(|| w.path.clone());
        }
    }

    let record_of = |name: &str| descriptions.get(name).and_then(|d| BranchRecord::parse(d));

    let mut branches = Vec::new();
    for (name, date) in &refs {
        if is_default(name) {
            continue;
        }
        let desc = descriptions.get(name);
        let record = desc.and_then(|d| BranchRecord::parse(d));
        let foreign_note =
            desc.is_some_and(|d| foreign_part(d).is_some_and(|f| !f.trim().is_empty()));
        let merged = merged_state(no_merged.as_ref(), name);
        branches.push(BranchInfo {
            name: name.clone(),
            record,
            foreign_note,
            merged,
            patches_on_default: None,
            patches_checked: false,
            ahead: (opts.content && merged == Some(true)).then_some(0),
            last_commit: date.clone(),
            worktree: branch_worktree.get(name).cloned(),
        });
    }
    branches.sort_by(|a, b| a.name.cmp(&b.name));

    // The patch check: the unmerged branches without a recorded state first, then the recorded
    // ones, name order within each, at most PATCH_CHECK_CAP of them (two git calls each).
    let mut patches_unchecked = 0;
    if let (true, Some(d)) = (opts.content, default.as_deref()) {
        let mut order: Vec<usize> = (0..branches.len())
            .filter(|&i| branches[i].merged == Some(false))
            .collect();
        order.sort_by(|&a, &b| {
            let (x, y) = (&branches[a], &branches[b]);
            (x.record.is_some(), &x.name).cmp(&(y.record.is_some(), &y.name))
        });
        patches_unchecked = order.len().saturating_sub(PATCH_CHECK_CAP);
        let base = format!("refs/heads/{d}");
        for &i in order.iter().take(PATCH_CHECK_CAP) {
            let b = &mut branches[i];
            let head = format!("refs/heads/{}", b.name);
            b.patches_on_default = git_out(repo, &["cherry", &base, &head])
                .map(|c| !c.lines().any(|l| l.trim_start().starts_with('+')));
            b.ahead = git_out(repo, &["rev-list", "--count", &format!("{base}..{head}")])
                .and_then(|s| s.trim().parse::<u64>().ok());
            b.patches_checked = true;
        }
    }

    // Worktrees: all but the primary and the default branch's one; bare entries skipped. C3's own
    // throwaway worktree of a kimi review is never a finding while a live run owns it; one no live
    // run owns is reported as stale.
    let mut wts = Vec::new();
    let mut stale_worktrees = Vec::new();
    for w in worktrees.iter().skip(1) {
        if w.bare || w.branch.as_deref().is_some_and(is_default) {
            continue;
        }
        if is_c3_throwaway_worktree(&w.path) {
            if let Some(why) = throwaway_stale_reason(Path::new(&w.path), SystemTime::now()) {
                stale_worktrees.push(StaleWorktree {
                    path: w.path.clone(),
                    why,
                });
            }
            continue;
        }
        let recorded = w.branch.as_deref().and_then(record_of).is_some();
        wts.push(WorktreeInfo {
            path: w.path.clone(),
            branch: w.branch.clone(),
            recorded,
            prunable: w.prunable,
        });
    }

    // The start folder (the primary worktree).
    let start = worktrees.first().filter(|w| !w.bare).map(|w| {
        let (behind_of, behind) = match default.as_deref() {
            Some(d) => {
                let behind_of = git_out(
                    repo,
                    &[
                        "rev-parse",
                        "--abbrev-ref",
                        "--symbolic-full-name",
                        &format!("{d}@{{upstream}}"),
                    ],
                )
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| d.to_string());
                let behind = if w.head.is_empty() {
                    None
                } else {
                    git_out(
                        repo,
                        &["rev-list", "--count", &format!("{}..{behind_of}", w.head)],
                    )
                    .and_then(|s| s.trim().parse::<u64>().ok())
                };
                (behind_of, behind)
            }
            None => (String::new(), None),
        };
        // Tracked files only: a C3-managed repository always holds untracked never-commit files
        // (raw event streams, detached status files), and a warning that is always on is not read.
        let dirty = if opts.dirty {
            git_out(
                Path::new(&w.path),
                &["status", "--porcelain", "--untracked-files=no"],
            )
            .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
        } else {
            None
        };
        StartFolder {
            path: w.path.clone(),
            branch: w.branch.clone(),
            on_default: w.branch.as_deref().is_some_and(is_default),
            behind,
            behind_of,
            dirty,
        }
    });

    Some(BranchReport {
        default_branch: default,
        default_error,
        ancestry_failed,
        branches,
        worktrees: wts,
        start,
        patches_unchecked,
        stale_worktrees,
    })
}

// ---------------------------------------------------------------------------------------------
// C3's throwaway worktrees (the kimi engine)
// ---------------------------------------------------------------------------------------------

/// Whether a directory name is exactly the one the kimi engine gives its throwaway worktree:
/// [`crate::engines::kimi::THROWAWAY_WORKTREE_PREFIX`] followed by the run's uuid (32 lower-case hex
/// digits).
pub fn is_throwaway_name(name: &str) -> bool {
    name.strip_prefix(crate::engines::kimi::THROWAWAY_WORKTREE_PREFIX)
        .is_some_and(|id| {
            id.len() == 32
                && id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}

/// Whether a worktree path is one C3 itself made for a kimi review: directly under the system temp
/// directory AND named exactly as [`is_throwaway_name`] says. A user's worktree that merely starts
/// with the prefix, or lives elsewhere, is an ordinary worktree.
fn is_c3_throwaway_worktree(path: &str) -> bool {
    let p = Path::new(path);
    let named = p
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(is_throwaway_name);
    named
        && p.parent()
            .is_some_and(|d| same_dir(d, &std::env::temp_dir()))
}

/// A worktree's git admin directory, read from its `.git` file (`gitdir: <path>`).
fn worktree_admin_dir(worktree: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(worktree.join(".git")).ok()?;
    let dir = text.lines().next()?.strip_prefix("gitdir:")?.trim();
    if dir.is_empty() {
        return None;
    }
    let p = PathBuf::from(dir);
    Some(if p.is_absolute() { p } else { worktree.join(p) })
}

/// Parse an owner record (`<pid> <start time>`; the start time may be empty).
pub fn parse_owner_record(text: &str) -> Option<(u32, String)> {
    let t = text.trim();
    let (pid, start) = t.split_once(char::is_whitespace).unwrap_or((t, ""));
    let pid = pid.parse::<u32>().ok().filter(|p| *p > 0)?;
    Some((pid, start.trim().to_string()))
}

/// (kimi engine) Record THIS process as the owner of a fresh throwaway worktree
/// ([`THROWAWAY_OWNER_FILE`] in its git admin directory). Best effort: without the record the
/// worktree counts as stale only once it is older than [`STALE_AFTER`].
pub(crate) fn record_throwaway_owner(worktree: &Path) {
    let Some(admin) = worktree_admin_dir(worktree) else {
        return;
    };
    let pid = std::process::id();
    let start = crate::liveness::proc::process_start_iso(pid).unwrap_or_default();
    let _ = std::fs::write(admin.join(THROWAWAY_OWNER_FILE), format!("{pid} {start}\n"));
}

/// The stale verdict, from what was found: `None` while a live run may own the worktree. Its
/// directory gone -> stale. An owner record -> stale exactly when that process (pid + start time)
/// is gone. No record -> stale once the directory is older than [`STALE_AFTER`].
pub fn stale_verdict(
    dir_exists: bool,
    owner: Option<(u32, String)>,
    owner_alive: impl Fn(u32, &str) -> bool,
    age: Option<Duration>,
) -> Option<String> {
    if !dir_exists {
        return Some("its directory is gone".to_string());
    }
    match owner {
        Some((pid, start)) => {
            (!owner_alive(pid, &start)).then(|| format!("the run that made it (pid {pid}) is gone"))
        }
        None => age
            .filter(|a| *a > STALE_AFTER)
            .map(|a| format!("no owner record and {} h old", a.as_secs() / 3600)),
    }
}

/// Why a throwaway worktree is stale (`None` while a live run may own it). The age is that of the
/// newer of the directory's creation and modification times.
fn throwaway_stale_reason(path: &Path, now: SystemTime) -> Option<String> {
    let meta = std::fs::metadata(path).ok();
    let owner = worktree_admin_dir(path)
        .and_then(|a| std::fs::read_to_string(a.join(THROWAWAY_OWNER_FILE)).ok())
        .and_then(|t| parse_owner_record(&t));
    let age = meta.as_ref().and_then(|m| {
        let newest = [m.created().ok(), m.modified().ok()]
            .into_iter()
            .flatten()
            .max()?;
        Some(now.duration_since(newest).unwrap_or_default())
    });
    stale_verdict(meta.is_some(), owner, crate::liveness::proc::pid_alive, age)
}

/// One stale worktree `c3 doctor --fix` tried to remove.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleRemoval {
    pub path: String,
    /// The directory is gone afterwards.
    pub removed: bool,
}

/// `c3 doctor --fix`: remove every stale throwaway worktree of a kimi review with the kimi engine's
/// own link-safe removal (links inside are unlinked first, never followed).
pub fn remove_stale_worktrees(repo_root: &Path) -> Vec<StaleRemoval> {
    let Some(report) = quick_scan(repo_root, false) else {
        return Vec::new();
    };
    report
        .stale_worktrees
        .iter()
        .map(|s| {
            let p = Path::new(&s.path);
            crate::engines::kimi::remove_throwaway_worktree(repo_root, p);
            StaleRemoval {
                path: s.path.clone(),
                removed: !p.exists(),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Wording shared by the listing, doctor, the hook and finish
// ---------------------------------------------------------------------------------------------

/// The command that records a state, for messages.
pub const SET_HINT: &str = "c3 branches --set <branch> --state work|waits|parked --note \"<text>\"";

/// `a, b, c` with at most `max` names, then `and N more`.
pub fn name_list(names: &[&str], max: usize) -> String {
    let mut s = names
        .iter()
        .take(max)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    if names.len() > max {
        s.push_str(&format!(" and {} more", names.len() - max));
    }
    s
}

/// The worktree's label: `<path> (<branch>)`, `<path> (detached)`, with `, prunable` when so.
pub fn worktree_label(w: &WorktreeInfo) -> String {
    let b = w.branch.as_deref().unwrap_or("detached");
    if w.prunable {
        format!("{} ({b}, prunable)", w.path)
    } else {
        format!("{} ({b})", w.path)
    }
}

/// The merge column of the listing.
pub fn merge_words(b: &BranchInfo, default: &str) -> String {
    match b.merged {
        Some(true) => "merged".to_string(),
        None => "merge state unknown".to_string(),
        Some(false) => match b.patches_on_default {
            Some(true) => format!(
                "not merged by ancestry, patches already on {default} (git cherry: patch equivalence, not the current content)"
            ),
            _ => "not merged".to_string(),
        },
    }
}

/// The start folder's branch, `detached` when none.
fn start_branch(s: &StartFolder) -> &str {
    s.branch.as_deref().unwrap_or("detached HEAD")
}

/// The SessionStart line (or `None` when there is nothing to say), from a report. `here` is true
/// when the session runs in the start folder itself ("this checkout").
pub fn hook_line_for(report: &BranchReport, here: bool) -> Option<String> {
    let nb = report.unknown_branches().len();
    let nw = report.unrecorded_worktrees().len();
    let mut parts = Vec::new();
    if report.default_branch.is_none() {
        parts.push("default branch unknown - c3 branches --default <name>".to_string());
    }
    if nb > 0 || nw > 0 {
        let mut what = Vec::new();
        if nb > 0 {
            what.push(format!("{nb} branch(es)"));
        }
        if nw > 0 {
            what.push(format!("{nw} worktree(s)"));
        }
        parts.push(format!(
            "{} without a recorded state - c3 branches",
            what.join(" and ")
        ));
    }
    if let (Some(s), Some(default)) = (&report.start, report.default_branch.as_deref()) {
        if !s.on_default {
            let who = if here {
                "this checkout".to_string()
            } else {
                format!("the start folder {}", s.path)
            };
            let mut l = format!("{who} is on {}", start_branch(s));
            if let Some(n) = s.behind.filter(|n| *n > 0) {
                l.push_str(&format!(", {n} behind {}", s.behind_of));
            }
            l.push_str(&format!(" (default {default})"));
            parts.push(l);
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("c3: {}", parts.join("; ")))
    }
}

/// The short note `c3 finish` prints after its commit (or `None` when nothing is unknown).
pub fn finish_note_for(report: &BranchReport) -> Option<String> {
    let ub = report.unknown_branches();
    let uw = report.unrecorded_worktrees();
    let mut notes = Vec::new();
    if let Some(why) = report.default_unknown_text() {
        notes.push(format!("Note: {why}."));
    }
    let mut what = Vec::new();
    if !ub.is_empty() {
        let names: Vec<&str> = ub.iter().map(|b| b.name.as_str()).collect();
        what.push(format!(
            "{} branch(es) ({})",
            ub.len(),
            name_list(&names, 8)
        ));
    }
    if !uw.is_empty() {
        let labels: Vec<String> = uw.iter().map(|w| worktree_label(w)).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        what.push(format!(
            "{} worktree(s) ({})",
            uw.len(),
            name_list(&refs, 8)
        ));
    }
    if !what.is_empty() {
        notes.push(format!(
            "Note: {} without a recorded state - run c3 branches, then integrate and remove them or record a state ({SET_HINT}).",
            what.join(" and ")
        ));
    }
    if notes.is_empty() {
        None
    } else {
        Some(notes.join(" "))
    }
}

// ---------------------------------------------------------------------------------------------
// The hook action and the finish note
// ---------------------------------------------------------------------------------------------

/// Whether the SessionStart branch check is ON. Default ON; `C3_BRANCH_CHECK=off` (environment
/// wins) or `"branch_check": false` in the C3 config turns it off.
pub fn enabled() -> bool {
    if let Ok(v) = std::env::var("C3_BRANCH_CHECK") {
        return !v.trim().eq_ignore_ascii_case("off");
    }
    match crate::c3config::load() {
        Ok(Some(cfg)) => cfg.branch_check,
        _ => true,
    }
}

/// Whether `repo_root` is inside a git repository (a `.git` dir, or a `.git` file for a worktree).
fn is_git_repo(repo_root: &Path) -> bool {
    repo_root.join(".git").exists()
}

/// Two paths name the same directory (canonicalised when possible; case-folded on Windows).
fn same_dir(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| -> String {
        let c = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let mut s = c.to_string_lossy().replace('\\', "/");
        if let Some(r) = s.strip_prefix("//?/") {
            s = r.to_string();
        }
        let s = s.trim_end_matches('/').to_string();
        if cfg!(windows) {
            s.to_lowercase()
        } else {
            s
        }
    };
    norm(a) == norm(b)
}

/// The `c3 hook` line: ONE line when a branch or worktree has no recorded state, the default branch
/// is unknown, or the start folder is not on the default branch; else `None`. Cheap (ancestry only,
/// no `git cherry`, no `git status`), read-only (no optional locks), every error swallowed; nothing
/// inside a C3 child, outside a git repository or with the check off.
pub fn hook_line() -> Option<String> {
    if !enabled() || crate::engines::in_c3_child() {
        return None;
    }
    let cwd = std::env::current_dir().ok()?;
    let repo_root = crate::providers::resolve_repo_root(&cwd);
    if !is_git_repo(&repo_root) {
        return None;
    }
    let report = scan(&repo_root, ScanOpts::default())?;
    let here = report
        .start
        .as_ref()
        .is_some_and(|s| same_dir(Path::new(&s.path), &repo_root));
    hook_line_for(&report, here)
}

/// The cheap report `c3 finish` and `c3 doctor` read (no `git cherry`); `None` outside a git
/// repository.
pub fn quick_scan(repo_root: &Path, dirty: bool) -> Option<BranchReport> {
    if !is_git_repo(repo_root) {
        return None;
    }
    scan(
        repo_root,
        ScanOpts {
            content: false,
            dirty,
        },
    )
}

// ---------------------------------------------------------------------------------------------
// --set / --clear
// ---------------------------------------------------------------------------------------------

/// Today's local date, `YYYY-MM-DD`.
pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// The owner default: `CODEX_CONSULT_COORDINATOR` when set, else empty.
pub fn default_owner() -> String {
    std::env::var("CODEX_CONSULT_COORDINATOR")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// The config key of a branch's description (the name must have passed [`check_branch_name`]).
fn description_key(branch: &str) -> String {
    format!("branch.{branch}.description")
}

/// The values of `git config -z --get-all` output, each followed by a NUL, exactly as stored.
/// `None` when the output is not NUL-framed.
pub fn nul_values(out: &str) -> Option<Vec<&str>> {
    if out.is_empty() {
        return Some(Vec::new());
    }
    Some(out.strip_suffix('\0')?.split('\0').collect())
}

/// A branch's description exactly as the repository's own config stores it (`git config --local -z
/// --get-all`: NUL framing, nothing trimmed). `Ok(None)` when there is none; `Err` when it cannot be
/// read exactly - git failed, the value is not UTF-8, the key holds several values - so a caller
/// never writes over a value it could not read.
pub fn read_description(repo: &Path, branch: &str) -> Result<Option<String>, String> {
    let key = description_key(branch);
    let out = git_cmd(repo)
        .args(["config", "--local", "-z", "--get-all", &key])
        .output()
        .map_err(|e| format!("could not run git config to read {key}: {e}"))?;
    match out.status.code() {
        Some(0) => {}
        // Exit 1 with no output: the key is not set (the name was validated, so the key is valid).
        Some(1) if out.stdout.is_empty() => return Ok(None),
        _ => {
            return Err(format!(
                "could not read {key}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        }
    }
    let text = String::from_utf8(out.stdout).map_err(|_| format!("{key} is not UTF-8 text"))?;
    let values =
        nul_values(&text).ok_or_else(|| format!("could not read {key}: no NUL framing"))?;
    match values.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some((*one).to_string())),
        many => Err(format!("{key} holds {} values", many.len())),
    }
}

/// Write one value to the repository's own config.
fn write_config(repo: &Path, key: &str, value: &str) -> Result<(), String> {
    let out = git_cmd(repo)
        .args(["config", "--local", key, value])
        .output()
        .map_err(|e| format!("git config: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git config could not write {key}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

/// Remove one value from the repository's own config.
fn unset_config(repo: &Path, key: &str) -> Result<(), String> {
    let out = git_cmd(repo)
        .args(["config", "--local", "--unset", key])
        .output()
        .map_err(|e| format!("git config: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git config could not remove {key}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

/// Re-read the description right before a write and refuse when it is no longer what was read:
/// another writer got in between, and C3 never writes over a change it has not seen.
fn unchanged_since(repo: &Path, branch: &str, read: &Option<String>) -> Result<(), String> {
    // (test only) Another writer between C3's read and this re-read.
    if let Some(v) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_BRANCH_DESC_RACE") {
        let _ = write_config(repo, &description_key(branch), &v);
    }
    let now = read_description(repo, branch).map_err(|e| format!("{e}; nothing written"))?;
    if &now != read {
        return Err(format!(
            "the description of '{branch}' changed while C3 was writing it; nothing written - run the command again"
        ));
    }
    Ok(())
}

/// Write a recorded state. Refuses an invalid branch name, an unknown branch, the default branch,
/// an empty note for `waits` / `parked`, a description that cannot be read exactly and one that
/// changes while C3 writes. A foreign description is kept, byte for byte, below the record line.
/// Returns the line written.
pub fn set_state(
    repo: &Path,
    branch: &str,
    state: BranchState,
    note: &str,
    owner: &str,
    date: &str,
) -> Result<String, String> {
    check_branch_name(repo, branch)?;
    if !branch_exists(repo, branch) {
        return Err(format!("no local branch named '{branch}'"));
    }
    if default_branch(repo).ok().as_deref() == Some(branch) {
        return Err(format!(
            "'{branch}' is the default branch; it needs no recorded state"
        ));
    }
    if matches!(state, BranchState::Waits | BranchState::Parked) && note.trim().is_empty() {
        return Err(match state {
            BranchState::Waits => {
                "--state waits needs --note with the question put to the operator".to_string()
            }
            _ => "--state parked needs --note with the condition for merging it".to_string(),
        });
    }
    let record = BranchRecord {
        state,
        owner: owner.to_string(),
        date: date.to_string(),
        text: note.to_string(),
    };
    let existing = read_description(repo, branch).map_err(|e| format!("{e}; nothing written"))?;
    let desc = description_with_record(existing.as_deref(), &record);
    unchanged_since(repo, branch, &existing)?;
    write_config(repo, &description_key(branch), &desc)?;
    Ok(record.format())
}

/// The outcome of [`clear_state`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClearOutcome {
    /// The record line was removed (a foreign note, if any, stays).
    Cleared,
    /// There was no description at all.
    NothingRecorded,
    /// The description is somebody's own note: kept untouched.
    ForeignKept,
}

/// Remove C3's record line from a branch description, restoring the foreign rest byte for byte;
/// never touches a foreign description (a `c3: ` line that is not a full record included). Refuses
/// like [`set_state`] when the description cannot be read exactly or changes meanwhile.
pub fn clear_state(repo: &Path, branch: &str) -> Result<ClearOutcome, String> {
    check_branch_name(repo, branch)?;
    if !branch_exists(repo, branch) {
        return Err(format!("no local branch named '{branch}'"));
    }
    let existing = read_description(repo, branch).map_err(|e| format!("{e}; nothing changed"))?;
    let Some(desc) = existing.as_deref() else {
        return Ok(ClearOutcome::NothingRecorded);
    };
    if !is_c3_description(desc) {
        return Ok(ClearOutcome::ForeignKept);
    }
    let rest = foreign_part(desc).map(str::to_string);
    unchanged_since(repo, branch, &existing)?;
    let key = description_key(branch);
    match rest {
        None => unset_config(repo, &key)?,
        Some(r) => write_config(repo, &key, &r)?,
    }
    Ok(ClearOutcome::Cleared)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(state: BranchState, owner: &str, text: &str) -> BranchRecord {
        BranchRecord {
            state,
            owner: owner.into(),
            date: "2026-11-18".into(),
            text: text.into(),
        }
    }

    #[test]
    fn record_round_trip() {
        for r in [
            rec(BranchState::Work, "claudecode/opus", "porting the hook"),
            rec(
                BranchState::Waits,
                "me",
                "merge web/fixes-e before the release?",
            ),
            rec(
                BranchState::Parked,
                "x",
                "after the full suite and the security review",
            ),
            rec(BranchState::Work, "x", ""),
        ] {
            let line = r.format();
            assert!(line.starts_with("c3: "), "{line}");
            assert_eq!(BranchRecord::parse(&line), Some(r.clone()), "{line}");
        }
        assert_eq!(
            rec(BranchState::Waits, "me", "q?").format(),
            "c3: waits | me | 2026-11-18 | q?"
        );
    }

    #[test]
    fn a_pipe_inside_the_text_survives() {
        let r = rec(BranchState::Parked, "o", "merge when a | b holds | c");
        let back = BranchRecord::parse(&r.format()).unwrap();
        assert_eq!(back.text, "merge when a | b holds | c");
        // A pipe in the owner would shift the fields: it is written as `/`.
        let r = rec(BranchState::Work, "a|b", "t");
        let back = BranchRecord::parse(&r.format()).unwrap();
        assert_eq!(back.owner, "a/b");
        assert_eq!(back.text, "t");
        // Line breaks collapse to one line.
        let r = rec(BranchState::Waits, "o", "line one\nline two");
        assert_eq!(
            BranchRecord::parse(&r.format()).unwrap().text,
            "line one line two"
        );
    }

    #[test]
    fn an_empty_owner_round_trips() {
        let r = rec(BranchState::Waits, "", "the question?");
        let line = r.format();
        assert_eq!(line, "c3: waits |  | 2026-11-18 | the question?");
        let back = BranchRecord::parse(&line).unwrap();
        assert_eq!(back.owner, "");
        assert_eq!(back, r);
        assert_eq!(back.describe(), "waits (2026-11-18): the question?");
    }

    #[test]
    fn a_foreign_description_is_not_a_record_and_is_kept() {
        let foreign = "my own note about this branch";
        assert_eq!(BranchRecord::parse(foreign), None);
        assert!(!is_c3_description(foreign));
        assert_eq!(foreign_part(foreign), Some(foreign));
        // Malformed c3 lines are not records either.
        assert_eq!(BranchRecord::parse("c3: bogus | a | b | c"), None);
        assert_eq!(BranchRecord::parse("c3: work | a"), None);
        // Setting a record keeps the foreign note below it; clearing restores it.
        let r = rec(BranchState::Work, "o", "t");
        let d = description_with_record(Some(foreign), &r);
        assert_eq!(d, format!("{}\n{foreign}", r.format()));
        assert_eq!(BranchRecord::parse(&d), Some(r.clone()));
        assert_eq!(foreign_part(&d), Some(foreign));
        // Re-setting replaces the record line only.
        let r2 = rec(BranchState::Waits, "o", "q?");
        let d2 = description_with_record(Some(&d), &r2);
        assert_eq!(d2, format!("{}\n{foreign}", r2.format()));
        // No description: the record alone; a record alone is replaced by the new one alone.
        assert_eq!(description_with_record(None, &r), r.format());
        assert_eq!(description_with_record(Some(&r.format()), &r2), r2.format());
        assert_eq!(foreign_part(&r.format()), None);
    }

    // (F02-7, F03-3, F04-9) a `c3: ` line that is not a full record is somebody's note; the foreign
    // part is kept byte for byte - trailing CRLFs, whitespace only, even empty.
    #[test]
    fn a_foreign_c3_line_and_exact_bytes_survive_set_and_clear() {
        let r = rec(BranchState::Parked, "o", "after review");
        for foreign in [
            "c3: hello",
            "c3: hello\r\nkeep me\r\n\r\n",
            "  \n ",
            "",
            "-starts with a dash",
            "note\r\n",
        ] {
            assert!(!is_c3_description(foreign), "{foreign:?}");
            let d = description_with_record(Some(foreign), &r);
            assert_eq!(BranchRecord::parse(&d), Some(r.clone()), "{foreign:?}");
            assert_eq!(foreign_part(&d), Some(foreign), "{foreign:?}");
            // Set again over our own record: the foreign bytes still unchanged.
            let d2 = description_with_record(Some(&d), &r);
            assert_eq!(d2, d, "{foreign:?}");
        }
        // A record line ending in CRLF is still C3's; the rest after its LF is the foreign part.
        let d = format!("{}\r\nnote", r.format());
        assert!(is_c3_description(&d));
        assert_eq!(foreign_part(&d), Some("note"));
    }

    #[test]
    fn nul_framed_values_are_exact() {
        assert_eq!(nul_values(""), Some(vec![]));
        assert_eq!(nul_values("\0"), Some(vec![""]));
        assert_eq!(nul_values("a\r\n\r\n\0"), Some(vec!["a\r\n\r\n"]));
        assert_eq!(nul_values("a\0b\0"), Some(vec!["a", "b"]));
        assert_eq!(nul_values("no framing"), None);
    }

    #[test]
    fn descriptions_parse_from_null_separated_config_output() {
        let out = "branch.feat/x.description\nc3: work | a | 2026-11-18 | t\0branch.v1.2.description\nline1\nline2\0branch.y.remote\norigin\0";
        let m = parse_descriptions(out);
        assert_eq!(m.len(), 2);
        assert_eq!(m["feat/x"], "c3: work | a | 2026-11-18 | t");
        assert_eq!(m["v1.2"], "line1\nline2");
    }

    #[test]
    fn worktrees_parse_from_porcelain() {
        let out = "worktree C:/repo\nHEAD aaa\nbranch refs/heads/pos-stage0\n\nworktree C:/tmp/wt1\nHEAD bbb\nbranch refs/heads/feat/a\n\nworktree C:/tmp/wt2\nHEAD ccc\ndetached\nprunable gitdir file points to non-existent location\n\n";
        let w = parse_worktrees(out);
        assert_eq!(w.len(), 3);
        assert_eq!(w[0].path, "C:/repo");
        assert_eq!(w[0].branch.as_deref(), Some("pos-stage0"));
        assert_eq!(w[1].branch.as_deref(), Some("feat/a"));
        assert_eq!(w[2].branch, None);
        assert!(w[2].prunable);
        assert_eq!(w[2].head, "ccc");
    }

    // (F02-9) a failed ancestry query is never "merged".
    #[test]
    fn a_failed_ancestry_query_leaves_branches_unknown() {
        let set: BTreeSet<String> = ["open".to_string()].into_iter().collect();
        assert_eq!(merged_state(Some(&set), "open"), Some(false));
        assert_eq!(merged_state(Some(&set), "done"), Some(true));
        assert_eq!(merged_state(None, "done"), None);
        let b = BranchInfo {
            name: "done".into(),
            merged: None,
            ..Default::default()
        };
        assert!(b.is_unknown());
        assert_eq!(merge_words(&b, "main"), "merge state unknown");
    }

    // (F02-11, F04-7) only the exact prefix + uuid name directly under the temp directory is C3's.
    #[test]
    fn only_the_exact_kimi_name_under_temp_is_a_throwaway() {
        let id = "0123456789abcdef0123456789abcdef";
        assert!(is_throwaway_name(&format!("c3-kimi-wt-{id}")));
        assert!(!is_throwaway_name("c3-kimi-wt-notes"));
        assert!(!is_throwaway_name(&format!("c3-kimi-wt-{}", &id[1..])));
        assert!(!is_throwaway_name(&format!(
            "c3-kimi-wt-{}",
            id.to_uppercase()
        )));
        assert!(!is_throwaway_name(&format!("c3-kimi-wt-{id}0")));
        let tmp = std::env::temp_dir();
        assert!(is_c3_throwaway_worktree(
            &tmp.join(format!("c3-kimi-wt-{id}")).to_string_lossy()
        ));
        assert!(!is_c3_throwaway_worktree(
            &tmp.join("c3-kimi-wt-notes").to_string_lossy()
        ));
        assert!(!is_c3_throwaway_worktree(
            &tmp.join("elsewhere")
                .join(format!("c3-kimi-wt-{id}"))
                .to_string_lossy()
        ));
    }

    #[test]
    fn the_stale_verdict_follows_the_owner_then_the_age() {
        let alive = |_: u32, _: &str| true;
        let dead = |_: u32, _: &str| false;
        let young = Some(Duration::from_secs(60));
        let old = Some(STALE_AFTER + Duration::from_secs(3600));
        // Directory gone: stale whatever else.
        assert!(stale_verdict(false, None, alive, young).is_some());
        // An owner record decides alone: a live owner keeps even an old worktree.
        assert_eq!(stale_verdict(true, Some((7, "s".into())), alive, old), None);
        let v = stale_verdict(true, Some((7, "s".into())), dead, young).unwrap();
        assert!(v.contains("pid 7"), "{v}");
        // No record: the age decides.
        assert_eq!(stale_verdict(true, None, dead, young), None);
        assert_eq!(stale_verdict(true, None, dead, None), None);
        assert!(stale_verdict(true, None, dead, old)
            .unwrap()
            .contains("7 h old"));
        assert_eq!(
            parse_owner_record("123 2026-11-22T10:00:00Z\n"),
            Some((123, "2026-11-22T10:00:00Z".into()))
        );
        assert_eq!(parse_owner_record("123 \n"), Some((123, String::new())));
        assert_eq!(parse_owner_record("0 x"), None);
        assert_eq!(parse_owner_record("junk"), None);
    }

    fn sample() -> BranchReport {
        BranchReport {
            default_branch: Some("main".into()),
            branches: vec![
                BranchInfo {
                    name: "done".into(),
                    merged: Some(true),
                    ..Default::default()
                },
                BranchInfo {
                    name: "picked".into(),
                    merged: Some(false),
                    patches_on_default: Some(true),
                    patches_checked: true,
                    ..Default::default()
                },
                BranchInfo {
                    name: "recorded".into(),
                    merged: Some(false),
                    record: Some(rec(BranchState::Parked, "o", "after review")),
                    ..Default::default()
                },
            ],
            worktrees: vec![
                WorktreeInfo {
                    path: "/t/wt".into(),
                    branch: Some("recorded".into()),
                    recorded: true,
                    prunable: false,
                },
                WorktreeInfo {
                    path: "/t/wt2".into(),
                    branch: None,
                    recorded: false,
                    prunable: false,
                },
            ],
            start: Some(StartFolder {
                path: "/repo".into(),
                branch: Some("pos-stage0".into()),
                on_default: false,
                behind: Some(303),
                behind_of: "origin/main".into(),
                dirty: None,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn unknown_is_unmerged_without_a_record() {
        let r = sample();
        let names: Vec<&str> = r
            .unknown_branches()
            .iter()
            .map(|b| b.name.as_str())
            .collect();
        assert_eq!(names, vec!["picked"]);
        assert_eq!(r.unrecorded_worktrees().len(), 1);
        assert!(!r.is_clean());
        assert_eq!(
            merge_words(&r.branches[1], "main"),
            "not merged by ancestry, patches already on main (git cherry: patch equivalence, not the current content)"
        );
        assert_eq!(merge_words(&r.branches[0], "main"), "merged");
        assert_eq!(merge_words(&r.branches[2], "main"), "not merged");
    }

    #[test]
    fn hook_line_says_what_is_unknown_and_where_the_start_folder_is() {
        let r = sample();
        assert_eq!(
            hook_line_for(&r, true).unwrap(),
            "c3: 1 branch(es) and 1 worktree(s) without a recorded state - c3 branches; this checkout is on pos-stage0, 303 behind origin/main (default main)"
        );
        let l = hook_line_for(&r, false).unwrap();
        assert!(l.contains("the start folder /repo is on pos-stage0"), "{l}");
        // Nothing to say -> no line.
        let mut clean = r.clone();
        clean.branches.retain(|b| !b.is_unknown());
        clean.worktrees.retain(|w| w.recorded);
        clean.start.as_mut().unwrap().on_default = true;
        assert_eq!(hook_line_for(&clean, true), None);
        assert_eq!(finish_note_for(&clean), None);
        let note = finish_note_for(&r).unwrap();
        assert!(note.contains("1 branch(es) (picked)"), "{note}");
        assert!(note.contains("/t/wt2 (detached)"), "{note}");
        // (F02-9) No default branch: said everywhere, never a start-folder claim against a guess.
        let mut unknown = clean.clone();
        unknown.default_branch = None;
        unknown.start.as_mut().unwrap().on_default = false;
        assert!(!unknown.is_clean());
        assert_eq!(
            hook_line_for(&unknown, true).unwrap(),
            "c3: default branch unknown - c3 branches --default <name>"
        );
        assert!(finish_note_for(&unknown)
            .unwrap()
            .contains("c3 branches --default <name>"));
    }

    #[test]
    fn name_list_caps_at_max() {
        let names = ["a", "b", "c"];
        assert_eq!(name_list(&names, 8), "a, b, c");
        assert_eq!(name_list(&names, 2), "a, b and 1 more");
    }
}
