//! The throwaway git worktree a reviewer runs in (release-0.2.5 wave B1, generalised from the kimi
//! engine's `KimiWorktree`).
//!
//! A fresh DETACHED worktree of one revision is made directly under the system temp directory
//! ([`ThrowawayWorktree::create`]); the reviewer child runs with it as its working directory, the
//! brief and the task's handoffs are staged into it at their repo-relative paths, and it is removed
//! when the guard drops - every success / failure / timeout path - with the link-safe removal of
//! [`remove_worktree_dir`] (every link inside is unlinked first and never followed; when that pass is
//! not clean nothing destructive runs and the directory is left for `c3 doctor --fix`). The owner
//! record in the worktree's git admin directory (`setup::branches::record_throwaway_owner`) lets
//! `c3 branches` / `c3 doctor` tell a crashed run's leftover from a live run's worktree.
//!
//! Who gets one (`consult::reviewed`): the kimi engine always (it has no tool-restriction flag, so
//! it must never run in the real checkout), every local-cwd engine under `c3 consult --rev <rev>`,
//! and a claude-engine reviewer on a `diff-review` / `acceptance` purpose (a frozen tree of HEAD).

use std::path::{Path, PathBuf};

/// The directory-name prefix of a reviewer's throwaway worktree (directly under the system temp
/// directory, followed by the run's 32-hex uuid). `setup::branches` leaves such a worktree out of its
/// "nobody owns this" count while a live run owns it, and reports it as stale once none does. The
/// name dates from the kimi engine, the first user; it is kept so the recognition of a leftover made
/// by an older build does not change.
pub const THROWAWAY_WORKTREE_PREFIX: &str = "c3-kimi-wt-";

/// Run a git subcommand in `repo`, returning trimmed stdout or an error.
pub(crate) fn git_capture(repo: &Path, args: &[&str]) -> Result<String, String> {
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

/// The full sha of the commit `rev` names in `repo` (`git rev-parse --verify <rev>^{commit}`). A
/// revision that starts with `-` is refused (it would be read as an option).
pub fn resolve_commit(repo: &Path, rev: &str) -> Result<String, String> {
    let rev = rev.trim();
    if rev.is_empty() || rev.starts_with('-') || rev.chars().any(char::is_whitespace) {
        return Err(format!("'{rev}' is not a revision"));
    }
    let sha = git_capture(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{commit}}"),
        ],
    )
    .map_err(|_| format!("'{rev}' is not a commit git knows in {}", repo.display()))?;
    if sha.len() < 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!(
            "'{rev}' is not a commit git knows in {}",
            repo.display()
        ));
    }
    Ok(sha)
}

/// (C3) The throwaway worktree a reviewer turn runs in (WRITE ISOLATION and a frozen view): a fresh
/// detached worktree of one commit under the OS temp is created on [`ThrowawayWorktree::create`],
/// the reviewer child runs with it as its `cwd`, and the worktree is removed when this guard drops,
/// with `git worktree prune` after - unless the removal cannot be made safely
/// ([`remove_worktree_dir`]), when the directory is left for `c3 doctor --fix`.
pub struct ThrowawayWorktree {
    repo: PathBuf,
    path: PathBuf,
    /// The full sha the worktree checks out.
    sha: String,
    /// The live tree had uncommitted changes when the worktree was made.
    main_dirty: bool,
    /// The outcome of the one removal attempt (`None`: not tried yet).
    removal: Option<Result<(), String>>,
}

impl ThrowawayWorktree {
    /// Make a fresh detached worktree of `rev` (a full sha, `HEAD` or any commit-ish) of `repo`
    /// under the system temp directory and record this process as its owner.
    pub fn create(repo: &Path, rev: &str) -> Result<ThrowawayWorktree, String> {
        let sha = resolve_commit(repo, rev)?;
        let main_dirty = !git_capture(repo, &["status", "--porcelain"])?.is_empty();
        let run_id = uuid::Uuid::new_v4().simple().to_string();
        let path = std::env::temp_dir().join(format!("{THROWAWAY_WORKTREE_PREFIX}{run_id}"));
        let path_s = path.to_string_lossy().to_string();
        git_capture(repo, &["worktree", "add", "--detach", &path_s, &sha])?;
        // Name this process as its owner (in the worktree's git admin directory, not its files), so
        // `c3 branches` / `c3 doctor` can tell a crashed run's leftover from a live run's worktree.
        crate::setup::branches::record_throwaway_owner(&path);
        Ok(ThrowawayWorktree {
            repo: repo.to_path_buf(),
            path,
            sha,
            main_dirty,
            removal: None,
        })
    }

    /// The worktree's directory (the reviewer's working directory).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The full sha the worktree checks out.
    pub fn sha(&self) -> &str {
        &self.sha
    }

    /// The handoff `Isolation:` header line: the worktree path and the full sha it checks out -
    /// `of HEAD <sha>` (with a note when the live tree was dirty at launch: the reviewer saw the
    /// committed HEAD only), or, for `--rev <rev>`, `of revision <sha> (--rev <rev>)`.
    pub fn isolation_line(&self, rev_given: Option<&str>) -> String {
        match rev_given {
            Some(rev) => format!(
                "Isolation: the reviewer ran in a throwaway git worktree `{}` of revision {} (--rev {rev}; removed after the run).",
                self.path.display(),
                self.sha
            ),
            None => {
                let mut l = format!(
                    "Isolation: the reviewer ran in a throwaway git worktree `{}` of HEAD {} (removed after the run).",
                    self.path.display(),
                    self.sha
                );
                if self.main_dirty {
                    l.push_str(
                        " The main working tree was dirty at launch; the reviewer saw the committed HEAD only.",
                    );
                }
                l
            }
        }
    }

    /// Copy the file `src` into the worktree at the repo-relative path `rel` (a [`stageable_rel`]
    /// path). Every directory on the way must be a real directory inside the worktree and the
    /// destination must not be a link, so a link the checked-out revision carries can never route
    /// the copy outside the worktree.
    pub fn stage_file(&self, src: &Path, rel: &str) -> Result<(), String> {
        if !stageable_rel(rel) {
            return Err(format!("`{rel}` is not a path inside the repository"));
        }
        let dest = self.path.join(rel);
        self.real_parent_inside(&dest)?;
        if let Ok(m) = std::fs::symlink_metadata(&dest) {
            if is_link(&m) {
                return Err(format!(
                    "`{rel}` is a link in the worktree; nothing is copied through it"
                ));
            }
        }
        std::fs::copy(src, &dest)
            .map(|_| ())
            .map_err(|e| format!("`{rel}` could not be copied ({e})"))
    }

    /// Copy the directory `src` (recursively; links are never followed or copied, a file `skip`
    /// names is left out) into the worktree at the repo-relative path `rel`. Returns how many files
    /// were copied.
    pub fn stage_dir(
        &self,
        src: &Path,
        rel: &str,
        skip: &dyn Fn(&Path) -> bool,
    ) -> Result<usize, String> {
        let mut n = 0usize;
        let mut stack: Vec<(PathBuf, String)> = vec![(src.to_path_buf(), rel.to_string())];
        while let Some((dir, dir_rel)) = stack.pop() {
            let rd = std::fs::read_dir(&dir)
                .map_err(|e| format!("{} cannot be listed ({e})", dir.display()))?;
            for entry in rd.flatten() {
                let p = entry.path();
                let meta = match std::fs::symlink_metadata(&p) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if is_link(&meta) {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                let child_rel = format!("{dir_rel}/{name}");
                if meta.is_dir() {
                    stack.push((p, child_rel));
                } else if meta.is_file() && !skip(&p) {
                    self.stage_file(&p, &child_rel)?;
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    /// Create the parent directory of `dest` (inside the worktree) one component at a time: an
    /// existing component must be a real directory, never a link (a link is never followed, so no
    /// directory is ever created outside the worktree), and the result must resolve inside the
    /// worktree's root.
    fn real_parent_inside(&self, dest: &Path) -> Result<(), String> {
        let parent = dest
            .parent()
            .ok_or_else(|| format!("{} has no parent", dest.display()))?;
        let rel = parent
            .strip_prefix(&self.path)
            .map_err(|_| format!("{} is not inside the worktree", parent.display()))?;
        let mut cur = self.path.clone();
        for c in rel.components() {
            cur.push(c);
            match std::fs::symlink_metadata(&cur) {
                Ok(m) if is_link(&m) => {
                    return Err(format!(
                        "{} is a link in the worktree; nothing is copied through it",
                        cur.display()
                    ))
                }
                Ok(m) if m.is_dir() => {}
                Ok(_) => return Err(format!("{} is not a directory", cur.display())),
                Err(_) => std::fs::create_dir(&cur)
                    .map_err(|e| format!("{} could not be created ({e})", cur.display()))?,
            }
        }
        let root = std::fs::canonicalize(&self.path)
            .map_err(|e| format!("the worktree root cannot be resolved ({e})"))?;
        let canon = std::fs::canonicalize(parent)
            .map_err(|e| format!("{} cannot be resolved ({e})", parent.display()))?;
        if !canon.starts_with(&root) {
            return Err(format!(
                "{} resolves outside the worktree",
                parent.display()
            ));
        }
        Ok(())
    }

    /// Remove the worktree once ([`remove_worktree_dir`] with the real git and unlink); a refusal
    /// is logged in one line and returned. A second call returns the first call's outcome.
    pub fn remove(&mut self) -> Result<(), String> {
        if let Some(r) = &self.removal {
            return r.clone();
        }
        let r = remove_worktree_dir(&self.repo, &self.path, &REAL_REMOVAL);
        if let Err(why) = &r {
            eprintln!(
                "codex-consult: the reviewer's throwaway worktree {} was left in place: {why} (`c3 doctor --fix` removes it once that is safe)",
                self.path.display()
            );
        }
        self.removal = Some(r.clone());
        r
    }
}

impl Drop for ThrowawayWorktree {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}

/// Removes ONE link (never its target) - [`remove_link`] in production.
type UnlinkFn = fn(&Path, &std::fs::Metadata) -> std::io::Result<()>;

/// The git runner and the link remover of a throwaway worktree removal: the real ones in
/// production ([`REAL_REMOVAL`]); a test passes a git recorder or an injected unlink failure.
struct RemovalOps {
    git: fn(&Path, &[&str]) -> Result<String, String>,
    unlink: UnlinkFn,
}

const REAL_REMOVAL: RemovalOps = RemovalOps {
    git: git_capture,
    unlink: remove_link,
};

/// Remove a throwaway worktree of `repo` WITHOUT ever reaching outside it (F02-21, F03-2, F04-5).
/// First every link inside is unlinked ([`unlink_links_inside_with`]: every reparse point counts
/// as a link and is never descended into). If that pass hit ANY error - a link that could not be
/// removed, an unreadable entry, a directory that is no longer a real directory inside the root -
/// or the root itself is (now) a link, NOTHING destructive runs: no `git worktree remove --force`,
/// no prune, no `remove_dir_all`; `Err` says why and the directory stays for a later `c3 doctor
/// --fix`. After a clean pass the root is checked once more and the worktree removed with git; the
/// directory fallback (when git left something) runs only after the same pass comes back clean
/// again. A path that does not exist at all is only pruned.
fn remove_worktree_dir(repo: &Path, path: &Path, ops: &RemovalOps) -> Result<(), String> {
    if let Err(e) = std::fs::symlink_metadata(path) {
        if e.kind() == std::io::ErrorKind::NotFound {
            let _ = (ops.git)(repo, &["worktree", "prune"]);
            return Ok(());
        }
    }
    let pass = unlink_links_inside_with(path, ops.unlink);
    if !pass.errors.is_empty() {
        return Err(pass.summary());
    }
    real_dir_canonical(path).map_err(|why| format!("the worktree root {why}"))?;
    let path_s = path.to_string_lossy().to_string();
    let _ = (ops.git)(repo, &["worktree", "remove", "--force", &path_s]);
    let _ = (ops.git)(repo, &["worktree", "prune"]);
    if std::fs::symlink_metadata(path).is_err() {
        return Ok(());
    }
    // `worktree remove` left something (or refused): the same checks again, then the directory.
    let again = unlink_links_inside_with(path, ops.unlink);
    if !again.errors.is_empty() {
        return Err(again.summary());
    }
    std::fs::remove_dir_all(path)
        .map_err(|e| format!("the directory could not be removed ({e})"))?;
    let _ = (ops.git)(repo, &["worktree", "prune"]);
    Ok(())
}

/// The result of one unlink pass: how many links were removed and every error met.
#[derive(Debug, Default)]
struct UnlinkPass {
    removed: usize,
    errors: Vec<String>,
}

impl UnlinkPass {
    /// One line: the first error and how many more there were.
    fn summary(&self) -> String {
        match self.errors.len() {
            0 => String::new(),
            1 => self.errors[0].clone(),
            n => format!("{} (and {} more error(s))", self.errors[0], n - 1),
        }
    }
}

/// Whether an entry's metadata (read WITHOUT following it) is a link: on Windows every reparse
/// point (`FILE_ATTRIBUTE_REPARSE_POINT` - a symlink, a junction, a mount point or any other
/// kind), the same test `panel/roles.rs` uses; elsewhere a symlink.
fn is_link(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

/// The canonical path of `p` when it is a real directory - not a link (read without following
/// it) and a directory; else why not.
fn real_dir_canonical(p: &Path) -> Result<PathBuf, String> {
    let meta = std::fs::symlink_metadata(p).map_err(|e| format!("cannot be read ({e})"))?;
    if is_link(&meta) {
        return Err("is a link".to_string());
    }
    if !meta.is_dir() {
        return Err("is not a directory".to_string());
    }
    std::fs::canonicalize(p).map_err(|e| format!("cannot be resolved ({e})"))
}

/// Remove every link INSIDE `root` without following it: `root` must be a real directory (not a
/// link); the walk reads each entry with `symlink_metadata`, removes a link ([`is_link`]: every
/// reparse point on Windows) with `unlink` and never descends into one. Before a directory is read,
/// and again before its entries are acted on, it is re-checked to still be a real directory whose
/// canonical path lies inside the root's - otherwise the walk ABORTS. Every other error (an
/// unreadable directory or entry, a link that could not be removed or is still there) is collected
/// and the walk goes on. `root` itself is not touched.
fn unlink_links_inside_with(root: &Path, unlink: UnlinkFn) -> UnlinkPass {
    let mut pass = UnlinkPass::default();
    let canon_root = match real_dir_canonical(root) {
        Ok(c) => c,
        Err(why) => {
            pass.errors.push(format!("the worktree root {why}"));
            return pass;
        }
    };
    let inside = |dir: &Path| -> Result<(), String> {
        match real_dir_canonical(dir) {
            Ok(c) if c.starts_with(&canon_root) => Ok(()),
            Ok(_) => Err(format!("{} resolves outside the worktree", dir.display())),
            Err(why) => Err(format!("{} {why}", dir.display())),
        }
    };
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Err(why) = inside(&dir) {
            pass.errors.push(why);
            return pass;
        }
        let rd = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) => {
                pass.errors
                    .push(format!("{} cannot be listed ({e})", dir.display()));
                continue;
            }
        };
        let mut entries: Vec<PathBuf> = Vec::new();
        for entry in rd {
            match entry {
                Ok(en) => entries.push(en.path()),
                Err(e) => pass.errors.push(format!(
                    "an entry of {} cannot be read ({e})",
                    dir.display()
                )),
            }
        }
        if let Err(why) = inside(&dir) {
            pass.errors.push(why);
            return pass;
        }
        for path in entries {
            let meta = match std::fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    pass.errors
                        .push(format!("{} cannot be read ({e})", path.display()));
                    continue;
                }
            };
            if is_link(&meta) {
                match unlink(&path, &meta) {
                    Ok(()) if std::fs::symlink_metadata(&path).is_err() => pass.removed += 1,
                    Ok(()) => pass
                        .errors
                        .push(format!("the link {} is still there", path.display())),
                    Err(e) => pass.errors.push(format!(
                        "the link {} could not be removed ({e})",
                        path.display()
                    )),
                }
            } else if meta.is_dir() {
                stack.push(path);
            }
        }
    }
    pass
}

/// Remove one link (never its target): on Windows a reparse point carrying the directory attribute
/// (a junction, a directory symlink) with `remove_dir`, any other with `remove_file` (each falling
/// back to the other); elsewhere `remove_file`. Neither call follows the link.
fn remove_link(path: &Path, meta: &std::fs::Metadata) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x10 != 0 {
            return std::fs::remove_dir(path).or_else(|_| std::fs::remove_file(path));
        }
        std::fs::remove_file(path).or_else(|_| std::fs::remove_dir(path))
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
        std::fs::remove_file(path)
    }
}

/// (`c3 doctor --fix`) Remove a throwaway worktree a crashed run left behind, with exactly the
/// removal a live run's guard runs on drop ([`remove_worktree_dir`]: links inside are unlinked
/// first, never followed, and nothing is deleted when that pass was not clean). The caller has
/// established that `path` is such a worktree of `repo` and that no live run owns it
/// (`setup::branches`). `true` only when the removal ran to the end; a refused one is `false`.
pub(crate) fn remove_throwaway_worktree(repo: &Path, path: &Path) -> bool {
    let mut wt = ThrowawayWorktree {
        repo: repo.to_path_buf(),
        path: path.to_path_buf(),
        sha: String::new(),
        main_dirty: false,
        removal: None,
    };
    wt.remove().is_ok()
}

/// (R7 / A1) Whether a repo-relative reference can be staged into the throwaway worktree at the
/// SAME path: a non-empty relative path with no `..` component (it then lies inside the worktree
/// root). An absolute reference resolves against the real tree from anywhere and needs no staging.
pub fn stageable_rel(rel: &str) -> bool {
    let p = Path::new(rel);
    !rel.is_empty()
        && p.is_relative()
        && !p
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// (A1) The preflight of a seat whose reviewer runs in a throwaway worktree of `rev` (`HEAD`, or
/// the full sha of `--rev`): the prompt tells the reviewer to read the brief at `brief_ref`,
/// relative to its working directory - the worktree. That path resolves there when the brief is
/// staged into the worktree (a stageable relative path whose source can be read) or is committed at
/// `rev`; an absolute reference resolves against the real file. Otherwise the reviewer would answer
/// without its brief (xelixir panel 47cf7308, 2026-11-12: "the required framing brief is missing"),
/// so the seat is refused before anything starts. The refusal text carries " is not usable:" - the
/// wording the panel's re-seat classifies as a preflight-class refusal
/// (`panel::run::is_preflight_refusal`), so a panel replaces the seat from its reserve. `rev_label`
/// names the revision in the message (`HEAD`, `revision <sha>`). `None` = the brief will be readable
/// (or there is none).
pub fn isolated_brief_refusal(
    repo: &Path,
    engine: &str,
    brief_ref: &str,
    brief_path: Option<&Path>,
    rev: &str,
    rev_label: &str,
) -> Option<String> {
    if brief_ref.is_empty() || Path::new(brief_ref).is_absolute() {
        return None;
    }
    let stageable = stageable_rel(brief_ref);
    let readable = match brief_path {
        Some(p) => std::fs::File::open(p)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        None => Err("no brief file was resolved".to_string()),
    };
    if stageable && readable.is_ok() {
        return None;
    }
    let in_rev = stageable
        && git_capture(
            repo,
            &[
                "cat-file",
                "-e",
                &format!("{rev}:{}", brief_ref.replace('\\', "/")),
            ],
        )
        .is_ok();
    if in_rev {
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
        "the {engine} seat is not usable: the brief `{brief_ref}` is neither committed at {rev_label} nor stageable into the reviewer's throwaway worktree of {rev_label} - {why}; nothing was started (commit the brief, place it inside the repository, or seat a reviewer that reads the live tree)"
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
    if !brief_ref.is_empty() && stageable_rel(brief_ref) {
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

/// (B1) The prompt paragraph of a reviewer under `--rev <rev>`: its working directory is a
/// throwaway worktree of that revision (the full sha), not the live checkout; the brief (when it is
/// a repo-relative path) and the task's handoffs (`task_rel`, when the collab directory lies in the
/// repository) were copied in at their paths.
pub fn rev_prompt_line(sha: &str, rev: &str, brief_ref: &str, task_rel: Option<&str>) -> String {
    let mut l = format!(
        "Isolation: your working directory is a throwaway git worktree of revision {sha} (`--rev {rev}`), not the live checkout: review the code as it is at that revision."
    );
    let brief = !brief_ref.is_empty() && stageable_rel(brief_ref);
    match (brief, task_rel) {
        (true, Some(t)) => l.push_str(&format!(
            " The brief `{brief_ref}` and this task's handoffs (`{t}/handoffs/`) were copied into it at their paths."
        )),
        (true, None) => l.push_str(&format!(
            " The brief `{brief_ref}` was copied into it at that path."
        )),
        (false, Some(t)) => l.push_str(&format!(
            " This task's handoffs (`{t}/handoffs/`) were copied into it at their paths."
        )),
        (false, None) => {}
    }
    l
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// A directory link at `link` to `target`: a junction on Windows (`mklink /J` needs no
    /// privilege, unlike a symlink there), a symlink elsewhere.
    fn make_dir_link(link: &Path, target: &Path) {
        #[cfg(windows)]
        {
            let ok = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            assert!(ok, "mklink /J failed");
        }
        #[cfg(not(windows))]
        std::os::unix::fs::symlink(target, link).unwrap();
        assert!(
            link.join("sentinel.txt").exists(),
            "the link does not resolve"
        );
    }

    #[test]
    fn unlink_removes_a_directory_link_without_following_it() {
        let (base, wt, outside) = link_scratch("dirlink");
        let link = wt.join("sub").join("node_modules");
        make_dir_link(&link, &outside);
        let pass = unlink_links_inside_with(&wt, remove_link);
        assert_eq!(pass.removed, 1);
        assert!(pass.errors.is_empty(), "{:?}", pass.errors);
        assert_link_gone_and_sentinel_kept(&base, &wt, &outside, &link);
    }

    thread_local! {
        /// The git calls a [`recording_git`] removal made on this test's thread.
        static GIT_CALLS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    /// A git recorder: notes the subcommand and does nothing.
    fn recording_git(_repo: &Path, args: &[&str]) -> Result<String, String> {
        GIT_CALLS.with(|c| c.borrow_mut().push(args.join(" ")));
        Ok(String::new())
    }

    fn git_calls() -> Vec<String> {
        GIT_CALLS.with(|c| c.borrow().clone())
    }

    /// An injected unlink failure (a link held open, a denied delete).
    fn failing_unlink(_path: &Path, _meta: &std::fs::Metadata) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "injected unlink failure",
        ))
    }

    // (F02-21, F04-5) a link that could not be removed stops the destructive cleanup: no forced
    // `worktree remove`, no prune, no directory removal - the worktree, its link and the link's
    // target are all left as they were.
    #[test]
    fn an_unlink_failure_prevents_the_forced_removal() {
        let (base, wt, outside) = link_scratch("unlink-fails");
        let link = wt.join("sub").join("node_modules");
        make_dir_link(&link, &outside);
        let ops = RemovalOps {
            git: recording_git,
            unlink: failing_unlink,
        };
        let r = remove_worktree_dir(&base, &wt, &ops);
        let why = r.expect_err("a failed unlink must refuse the removal");
        assert!(why.contains("could not be removed"), "{why}");
        assert!(
            git_calls().is_empty(),
            "no git call after a failed unlink: {:?}",
            git_calls()
        );
        assert!(
            wt.join("sub").join("own.txt").exists(),
            "the worktree was left"
        );
        assert!(
            std::fs::symlink_metadata(&link).is_ok(),
            "the link was left"
        );
        assert!(outside.join("sentinel.txt").exists());
        // the same tree with the real unlink is removed cleanly (git still recorded)
        let ok = RemovalOps {
            git: recording_git,
            unlink: remove_link,
        };
        assert_eq!(remove_worktree_dir(&base, &wt, &ok), Ok(()));
        assert!(
            git_calls()
                .iter()
                .any(|c| c.starts_with("worktree remove --force")),
            "{:?}",
            git_calls()
        );
        assert!(
            std::fs::symlink_metadata(&wt).is_err(),
            "the fallback removed it"
        );
        assert!(outside.join("sentinel.txt").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    // (F02-21, F03-2) a worktree root that is (now) a link is refused before anything is touched:
    // nothing is unlinked or deleted through it, git is never called, the doctor wrapper reports it
    // as not removed.
    #[test]
    fn a_root_replaced_by_a_link_is_refused() {
        let (base, wt, outside) = link_scratch("root-link");
        let inner = outside.join("inner-link");
        make_dir_link(&inner, &outside);
        std::fs::remove_dir_all(&wt).unwrap();
        make_dir_link(&wt, &outside);
        let ops = RemovalOps {
            git: recording_git,
            unlink: remove_link,
        };
        let why = remove_worktree_dir(&base, &wt, &ops).expect_err("a link root is refused");
        assert!(why.contains("the worktree root is a link"), "{why}");
        assert!(git_calls().is_empty(), "{:?}", git_calls());
        assert!(!remove_throwaway_worktree(&base, &wt));
        assert!(outside.join("sentinel.txt").exists());
        assert!(
            std::fs::symlink_metadata(&inner).is_ok(),
            "a link behind the root link was not touched"
        );
        assert!(
            std::fs::symlink_metadata(&wt).is_ok(),
            "the root link was left"
        );
        // clean up the links themselves, never their target
        let _ = remove_link(&inner, &std::fs::symlink_metadata(&inner).unwrap());
        let _ = remove_link(&wt, &std::fs::symlink_metadata(&wt).unwrap());
        assert!(outside.join("sentinel.txt").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    // The production removal end to end on a real git worktree: a reviewer left a directory link to
    // an outside sentinel; the guard unlinks it, `git worktree remove --force` removes the worktree,
    // and the sentinel survives.
    #[test]
    fn a_real_worktree_with_a_link_to_an_outside_sentinel_is_removed_safely() {
        let repo = brief_repo("wt-remove");
        let outside = repo.parent().unwrap().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("sentinel.txt"), "keep me\n").unwrap();
        let mut wt = ThrowawayWorktree::create(&repo, "HEAD").unwrap();
        let path = wt.path.clone();
        let link = path.join("node_modules");
        make_dir_link(&link, &outside);
        assert_eq!(wt.remove(), Ok(()));
        assert!(
            std::fs::symlink_metadata(&path).is_err(),
            "the worktree is gone"
        );
        assert!(
            outside.join("sentinel.txt").exists(),
            "the sentinel survived"
        );
        let list = git_capture(&repo, &["worktree", "list", "--porcelain"]).unwrap();
        assert!(!list.contains(THROWAWAY_WORKTREE_PREFIX), "{list}");
        // a second call (the Drop) returns the first outcome without another attempt
        assert_eq!(wt.remove(), Ok(()));
        drop(wt);
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
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

    // (B1) a worktree of an OLDER revision checks out exactly that sha, names it in the handoff line,
    // takes a staged file and a staged directory (links and skipped files left out), refuses a
    // revision git does not know or one that reads as an option, and is gone after the guard drops.
    #[test]
    fn a_worktree_of_an_older_revision_checks_out_that_sha_and_takes_staged_files() {
        let repo = brief_repo("rev");
        let first = git_capture(&repo, &["rev-parse", "HEAD"]).unwrap();
        std::fs::write(repo.join("committed.md"), "# second\n").unwrap();
        git_capture(&repo, &["commit", "-q", "-am", "second"]).unwrap();
        assert_eq!(resolve_commit(&repo, &first[..10]).unwrap(), first);
        assert!(resolve_commit(&repo, "no-such-ref").is_err());
        assert!(resolve_commit(&repo, "--all").is_err());
        assert!(resolve_commit(&repo, "a b").is_err());

        let wt = ThrowawayWorktree::create(&repo, &first).unwrap();
        let path = wt.path().to_path_buf();
        assert_eq!(wt.sha(), first);
        assert_eq!(
            git_capture(&path, &["rev-parse", "HEAD"]).unwrap(),
            first,
            "the worktree checks out the asked revision"
        );
        // (the checkout may carry CRLF under a global core.autocrlf)
        assert_eq!(
            std::fs::read_to_string(path.join("committed.md"))
                .unwrap()
                .replace("\r\n", "\n"),
            "# c\n"
        );
        let line = wt.isolation_line(Some("v1"));
        assert!(
            line.contains(&format!("of revision {first} (--rev v1;")),
            "{line}"
        );
        assert!(wt
            .isolation_line(None)
            .contains(&format!("of HEAD {first} (removed after the run).")));

        // a file and a directory staged at their repo-relative paths
        let src = repo.parent().unwrap().join("src-handoffs");
        std::fs::create_dir_all(src.join("deep")).unwrap();
        std::fs::write(src.join("01-claude-x.md"), "brief\n").unwrap();
        std::fs::write(src.join("01-claude-x.events.jsonl"), "{}\n").unwrap();
        std::fs::write(src.join("deep").join("note.md"), "deep\n").unwrap();
        let skip = |p: &Path| p.to_string_lossy().ends_with(".events.jsonl");
        let n = wt.stage_dir(&src, ".collab/t/handoffs", &skip).unwrap();
        assert_eq!(n, 2);
        assert!(path.join(".collab/t/handoffs/01-claude-x.md").is_file());
        assert!(path.join(".collab/t/handoffs/deep/note.md").is_file());
        assert!(!path
            .join(".collab/t/handoffs/01-claude-x.events.jsonl")
            .exists());
        wt.stage_file(&src.join("01-claude-x.md"), "docs/brief.md")
            .unwrap();
        assert!(path.join("docs/brief.md").is_file());
        assert!(wt
            .stage_file(&src.join("01-claude-x.md"), "../out.md")
            .is_err());
        drop(wt);
        assert!(
            std::fs::symlink_metadata(&path).is_err(),
            "the worktree is gone after the guard drops"
        );
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }

    // (A1, B1) the preflight of a worktree-isolated seat: refused only when the brief can reach the
    // worktree neither by staging nor through the revision; the refusal is preflight-class (re-seated).
    #[test]
    fn the_isolated_brief_preflight_refuses_only_an_unreachable_brief() {
        assert!(stageable_rel(".collab/t/handoffs/01.md"));
        assert!(!stageable_rel("../notes/brief.md"));
        assert!(!stageable_rel(""));
        let repo = brief_repo("pre");
        let refusal =
            |b: &str, p: Option<&Path>| isolated_brief_refusal(&repo, "kimi", b, p, "HEAD", "HEAD");
        // no brief, an absolute brief: nothing to refuse.
        assert!(refusal("", None).is_none());
        let abs = repo.join("committed.md");
        let abs_s = abs.to_string_lossy().to_string();
        assert!(refusal(&abs_s, Some(&abs)).is_none());
        // an UNTRACKED brief inside the repository is staged: not refused.
        std::fs::create_dir_all(repo.join(".collab/t/handoffs")).unwrap();
        let untracked = repo.join(".collab/t/handoffs/01-claude-framing.md");
        std::fs::write(&untracked, "# brief\n").unwrap();
        assert!(refusal(".collab/t/handoffs/01-claude-framing.md", Some(&untracked)).is_none());
        // a committed brief is in HEAD even when its source cannot be opened.
        assert!(refusal("committed.md", None).is_none());
        // a brief outside the repository named by a relative path: neither in HEAD nor stageable.
        let outside = repo.parent().unwrap().join("notes");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("brief.md"), "# b\n").unwrap();
        let r = refusal("../notes/brief.md", Some(&outside.join("brief.md")))
            .expect("an unreachable brief is refused");
        assert!(r.starts_with("the kimi seat is not usable: the brief `../notes/brief.md`"));
        assert!(r.contains("neither committed at HEAD nor stageable"), "{r}");
        assert!(r.contains("nothing was started"), "{r}");
        assert!(crate::panel::run::is_preflight_refusal(&r), "{r}");
        // an untracked in-repo brief that cannot be resolved (no source) and is not in HEAD.
        let r2 = refusal("missing.md", None).unwrap();
        assert!(r2.contains("cannot be read"), "{r2}");
        // under --rev the revision is named, and a brief committed there is found.
        let head = git_capture(&repo, &["rev-parse", "HEAD"]).unwrap();
        let label = format!("revision {}", &head[..12]);
        assert!(
            isolated_brief_refusal(&repo, "claude", "committed.md", None, &head, &label).is_none()
        );
        let r3 =
            isolated_brief_refusal(&repo, "claude", "missing.md", None, &head, &label).unwrap();
        assert!(
            r3.contains(&format!("neither committed at {label} nor stageable")),
            "{r3}"
        );
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

    // (B1) the --rev paragraph names the full sha and what was copied in.
    #[test]
    fn the_rev_prompt_names_the_revision_and_the_staged_files() {
        let l = rev_prompt_line(
            "abc",
            "main~1",
            ".collab/t/handoffs/01.md",
            Some(".collab/t"),
        );
        assert!(l.starts_with("Isolation: your working directory is a throwaway git worktree of revision abc (`--rev main~1`), not the live checkout"), "{l}");
        assert!(l.ends_with("The brief `.collab/t/handoffs/01.md` and this task's handoffs (`.collab/t/handoffs/`) were copied into it at their paths."), "{l}");
        let none = rev_prompt_line("abc", "v1", "", None);
        assert!(
            none.ends_with("review the code as it is at that revision."),
            "{none}"
        );
    }
}
