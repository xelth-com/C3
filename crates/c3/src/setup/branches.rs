//! Branch and worktree state (`c3 branches`, and the checks in `c3 doctor`, the SessionStart hook
//! and `c3 finish`).
//!
//! A branch's state is ONE line stored as its git description
//! (`git config branch.<name>.description`): `c3: <state> | <owner> | <YYYY-MM-DD> | <text>`, where
//! `<state>` is `work` (in work; the owner names the session or person), `waits` (finished, waits
//! for the operator; the text is the question put to him) or `parked` (a WIP put aside; the text is
//! the condition for merging it). The description is shared by every worktree of the repository and
//! disappears with `git branch -d`. A description that does not start with `c3: ` is somebody's own
//! note: it is kept (a recorded state is written as the FIRST line above it, and `--clear` removes
//! only that line) and the branch counts as having no recorded state.
//!
//! A branch is **unknown** when it is not merged by ancestry into the default branch AND has no
//! recorded state. Everything here only READS the repository, except [`set_state`] and
//! [`clear_state`], which write the description and nothing else. Nothing is ever deleted or merged.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

/// The prefix that marks a description as C3's record.
pub const RECORD_PREFIX: &str = "c3: ";

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

/// Whether a description is C3's record (its first line starts with `c3: `).
pub fn is_c3_description(description: &str) -> bool {
    description.starts_with(RECORD_PREFIX)
}

/// The description with C3's record line removed (the foreign rest, possibly empty). A description
/// that is not C3's is returned unchanged.
pub fn foreign_part(description: &str) -> String {
    if !is_c3_description(description) {
        return description.to_string();
    }
    match description.split_once('\n') {
        Some((_, rest)) => rest.to_string(),
        None => String::new(),
    }
}

/// The new description for `--set`: the record line on top of the foreign part (kept as it was).
pub fn description_with_record(existing: &str, record: &BranchRecord) -> String {
    let foreign = foreign_part(existing);
    if foreign.trim().is_empty() {
        record.format()
    } else {
        format!("{}\n{}", record.format(), foreign)
    }
}

// ---------------------------------------------------------------------------------------------
// git plumbing
// ---------------------------------------------------------------------------------------------

/// Run git in `repo`; stdout on success, `None` on any failure (spawn error or non-zero exit).
fn git_out(repo: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Run git in `repo`; true when it exits 0.
fn git_ok(repo: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo)
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

/// The repository's default branch: `origin/HEAD` (with `origin/` stripped, when that local branch
/// exists), else `main` if it exists, else `master`, else the current branch. `None` when none of
/// these resolves (an empty repository, a detached HEAD with no main/master).
pub fn default_branch(repo: &Path) -> Option<String> {
    if let Some(s) = git_out(
        repo,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    ) {
        let s = s.trim();
        let name = s.strip_prefix("origin/").unwrap_or(s);
        if branch_exists(repo, name) {
            return Some(name.to_string());
        }
    }
    for name in ["main", "master"] {
        if branch_exists(repo, name) {
            return Some(name.to_string());
        }
    }
    let cur = git_out(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let cur = cur.trim();
    if cur.is_empty() {
        None
    } else {
        Some(cur.to_string())
    }
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

/// Every branch description of the repository (one `git config` call; empty on any error).
fn read_descriptions(repo: &Path) -> BTreeMap<String, String> {
    git_out(
        repo,
        &[
            "config",
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
    /// Merged into the default branch by ancestry.
    pub merged: bool,
    /// Not merged by ancestry, but every commit's content is already on the default branch
    /// (`git cherry` prints no `+` line: the cherry-pick case). Only computed by a full scan.
    pub content_on_default: Option<bool>,
    /// Commits ahead of the default branch. Only computed by a full scan.
    pub ahead: Option<u64>,
    /// Date of the last commit (`YYYY-MM-DD`).
    pub last_commit: String,
    /// The worktree that has it checked out, if any.
    pub worktree: Option<String>,
}

impl BranchInfo {
    /// Not merged by ancestry AND no recorded state.
    pub fn is_unknown(&self) -> bool {
        !self.merged && self.record.is_none()
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

/// The start folder: the primary worktree.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StartFolder {
    pub path: String,
    /// `None`: detached HEAD.
    pub branch: Option<String>,
    pub on_default: bool,
    /// Commits behind `behind_of`, as the local refs know it (no fetch).
    pub behind: Option<u64>,
    /// The default branch's upstream (`origin/main`), else the default branch itself.
    pub behind_of: String,
    /// Uncommitted changes (a count). Only computed when asked for.
    pub dirty: Option<usize>,
}

/// Everything a scan found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BranchReport {
    pub default_branch: String,
    pub branches: Vec<BranchInfo>,
    pub worktrees: Vec<WorktreeInfo>,
    pub start: Option<StartFolder>,
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

    /// Nothing unknown: every listed branch is merged or recorded, every worktree recorded.
    pub fn is_clean(&self) -> bool {
        self.unknown_branches().is_empty() && self.unrecorded_worktrees().is_empty()
    }
}

/// How much a scan computes.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScanOpts {
    /// Per unmerged branch: `git cherry` (content on default) and the ahead count. The full
    /// listing only; the hook never does this.
    pub content: bool,
    /// The start folder's uncommitted-change count (`git status --porcelain`).
    pub dirty: bool,
}

/// Scan the repository at `repo`. `None` outside a git repository or when no default branch
/// resolves; every other git error degrades to missing data, never to a failure.
pub fn scan(repo: &Path, opts: ScanOpts) -> Option<BranchReport> {
    let default = default_branch(repo)?;

    // Every local branch with its last commit date (one call), and the unmerged ones (one call).
    let all = git_out(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname)%09%(committerdate:short)",
            "refs/heads",
        ],
    )?;
    let no_merged: BTreeSet<String> = git_out(
        repo,
        &[
            "for-each-ref",
            &format!("--no-merged=refs/heads/{default}"),
            "--format=%(refname)",
            "refs/heads",
        ],
    )
    .unwrap_or_default()
    .lines()
    .filter_map(|l| l.trim().strip_prefix("refs/heads/").map(str::to_string))
    .collect();

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
    for line in all.lines() {
        let (refname, date) = line.split_once('\t').unwrap_or((line, ""));
        let Some(name) = refname.trim().strip_prefix("refs/heads/") else {
            continue;
        };
        if name == default {
            continue;
        }
        let desc = descriptions.get(name);
        let record = desc.and_then(|d| BranchRecord::parse(d));
        let foreign_note = desc.is_some_and(|d| !foreign_part(d).trim().is_empty());
        let merged = !no_merged.contains(name);
        let (content_on_default, ahead) = if opts.content {
            if merged {
                (None, Some(0))
            } else {
                let cherry = git_out(repo, &["cherry", &default, name]);
                let content = cherry.map(|c| !c.lines().any(|l| l.trim_start().starts_with('+')));
                let ahead = git_out(
                    repo,
                    &[
                        "rev-list",
                        "--count",
                        &format!("{default}..refs/heads/{name}"),
                    ],
                )
                .and_then(|s| s.trim().parse::<u64>().ok());
                (content, ahead)
            }
        } else {
            (None, None)
        };
        branches.push(BranchInfo {
            name: name.to_string(),
            record,
            foreign_note,
            merged,
            content_on_default,
            ahead,
            last_commit: date.trim().to_string(),
            worktree: branch_worktree.get(name).cloned(),
        });
    }
    branches.sort_by(|a, b| a.name.cmp(&b.name));

    // Worktrees: all but the primary and the default branch's one; bare entries skipped.
    let mut wts = Vec::new();
    for w in worktrees.iter().skip(1) {
        if w.bare || w.branch.as_deref() == Some(default.as_str()) {
            continue;
        }
        // C3's own throwaway worktree of a running kimi review is owned by that run and removed by
        // it; counting it would make every kimi review a finding here.
        if is_c3_throwaway_worktree(&w.path) {
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
        let behind_of = git_out(
            repo,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                &format!("{default}@{{upstream}}"),
            ],
        )
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.clone());
        let behind = if w.head.is_empty() {
            None
        } else {
            git_out(
                repo,
                &["rev-list", "--count", &format!("{}..{behind_of}", w.head)],
            )
            .and_then(|s| s.trim().parse::<u64>().ok())
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
            on_default: w.branch.as_deref() == Some(default.as_str()),
            behind,
            behind_of,
            dirty,
        }
    });

    Some(BranchReport {
        default_branch: default,
        branches,
        worktrees: wts,
        start,
    })
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

/// Whether a worktree path is one C3 itself made for a kimi review (`engines::kimi`): its directory
/// name carries [`crate::engines::kimi::THROWAWAY_WORKTREE_PREFIX`].
fn is_c3_throwaway_worktree(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(crate::engines::kimi::THROWAWAY_WORKTREE_PREFIX))
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
    if b.merged {
        "merged".to_string()
    } else {
        match b.content_on_default {
            Some(true) => format!(
                "not merged by ancestry, content on {default} (integrated by cherry-pick or squash)"
            ),
            _ => "not merged".to_string(),
        }
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
    if let Some(s) = &report.start {
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
            l.push_str(&format!(" (default {})", report.default_branch));
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
    if ub.is_empty() && uw.is_empty() {
        return None;
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
    Some(format!(
        "Note: {} without a recorded state - run c3 branches, then integrate and remove them or record a state ({SET_HINT}).",
        what.join(" and ")
    ))
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

/// The `c3 hook` line: ONE line when a branch or worktree has no recorded state, or the start
/// folder is not on the default branch; else `None`. Cheap (ancestry only, no `git cherry`, no
/// `git status`), read-only, every error swallowed; nothing inside a C3 child, outside a git
/// repository or with the check off.
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

/// The description of a branch (`None` when there is none).
fn read_description(repo: &Path, branch: &str) -> Option<String> {
    git_out(
        repo,
        &["config", "--get", &format!("branch.{branch}.description")],
    )
    .map(|s| s.trim_end_matches(['\r', '\n']).to_string())
}

/// Write a recorded state. Refuses an unknown branch, the default branch, and an empty note for
/// `waits` / `parked`. A foreign description is kept below the record line. Returns the line
/// written.
pub fn set_state(
    repo: &Path,
    branch: &str,
    state: BranchState,
    note: &str,
    owner: &str,
    date: &str,
) -> Result<String, String> {
    if !branch_exists(repo, branch) {
        return Err(format!("no local branch named '{branch}'"));
    }
    if default_branch(repo).as_deref() == Some(branch) {
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
    let existing = read_description(repo, branch).unwrap_or_default();
    let desc = description_with_record(&existing, &record);
    let key = format!("branch.{branch}.description");
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["config", &key, &desc])
        .output()
        .map_err(|e| format!("git config: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git config failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
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

/// Remove C3's record line from a branch description; never touches a foreign one.
pub fn clear_state(repo: &Path, branch: &str) -> Result<ClearOutcome, String> {
    if !branch_exists(repo, branch) {
        return Err(format!("no local branch named '{branch}'"));
    }
    let Some(existing) = read_description(repo, branch) else {
        return Ok(ClearOutcome::NothingRecorded);
    };
    if !is_c3_description(&existing) {
        return Ok(ClearOutcome::ForeignKept);
    }
    let rest = foreign_part(&existing);
    let key = format!("branch.{branch}.description");
    let ok = if rest.trim().is_empty() {
        git_ok(repo, &["config", "--unset", &key])
    } else {
        git_ok(repo, &["config", &key, &rest])
    };
    if ok {
        Ok(ClearOutcome::Cleared)
    } else {
        Err(format!("git config could not change {key}"))
    }
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
        assert_eq!(foreign_part(foreign), foreign);
        // Malformed c3 lines are not records either.
        assert_eq!(BranchRecord::parse("c3: bogus | a | b | c"), None);
        assert_eq!(BranchRecord::parse("c3: work | a"), None);
        // Setting a record keeps the foreign note below it; clearing restores it.
        let r = rec(BranchState::Work, "o", "t");
        let d = description_with_record(foreign, &r);
        assert_eq!(d, format!("{}\n{foreign}", r.format()));
        assert_eq!(BranchRecord::parse(&d), Some(r.clone()));
        assert_eq!(foreign_part(&d), foreign);
        // Re-setting replaces the record line only.
        let r2 = rec(BranchState::Waits, "o", "q?");
        let d2 = description_with_record(&d, &r2);
        assert_eq!(d2, format!("{}\n{foreign}", r2.format()));
        // No foreign part: the record alone.
        assert_eq!(description_with_record("", &r), r.format());
        assert_eq!(description_with_record(&r.format(), &r2), r2.format());
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

    fn sample() -> BranchReport {
        BranchReport {
            default_branch: "main".into(),
            branches: vec![
                BranchInfo {
                    name: "done".into(),
                    merged: true,
                    ..Default::default()
                },
                BranchInfo {
                    name: "picked".into(),
                    content_on_default: Some(true),
                    ..Default::default()
                },
                BranchInfo {
                    name: "recorded".into(),
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
            "not merged by ancestry, content on main (integrated by cherry-pick or squash)"
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
    }

    #[test]
    fn name_list_caps_at_max() {
        let names = ["a", "b", "c"];
        assert_eq!(name_list(&names, 8), "a, b, c");
        assert_eq!(name_list(&names, 2), "a, b and 1 more");
    }
}
