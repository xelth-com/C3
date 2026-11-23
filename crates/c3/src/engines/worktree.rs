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
//! `c3 branches` / `c3 doctor` tell a crashed run's leftover from a live run's worktree; a worktree
//! whose record cannot be written is removed again and not used.
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
        Self::create_with(repo, rev, crate::setup::branches::record_throwaway_owner)
    }

    /// [`ThrowawayWorktree::create`] with the owner recorder given (a test injects a failing one).
    fn create_with(
        repo: &Path,
        rev: &str,
        record_owner: fn(&Path) -> Result<(), String>,
    ) -> Result<ThrowawayWorktree, String> {
        let sha = resolve_commit(repo, rev)?;
        let main_dirty = !git_capture(repo, &["status", "--porcelain"])?.is_empty();
        let run_id = uuid::Uuid::new_v4().simple().to_string();
        let path = std::env::temp_dir().join(format!("{THROWAWAY_WORKTREE_PREFIX}{run_id}"));
        let path_s = path.to_string_lossy().to_string();
        git_capture(repo, &["worktree", "add", "--detach", &path_s, &sha])?;
        // Name this process as its owner (in the worktree's git admin directory, not its files), so
        // `c3 branches` / `c3 doctor` can tell a crashed run's leftover from a live run's worktree.
        // (0.2.5 R2, F13-8) Not best effort: a worktree whose record cannot be written could not be
        // told from a leftover, so it is not used - it is removed (link-safely) and the run fails as
        // one whose worktree could not be made.
        if let Err(why) = record_owner(&path) {
            let removal = match remove_worktree_dir(repo, &path, &REAL_REMOVAL) {
                Ok(()) => "the worktree was removed".to_string(),
                Err(e) => format!(
                    "the worktree was left in place ({e}); `c3 doctor` reports it for removal by hand"
                ),
            };
            return Err(format!(
                "its owner record could not be written ({why}); {removal}"
            ));
        }
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
    /// path): its bytes are read first, then written with [`ThrowawayWorktree::stage_bytes`].
    pub fn stage_file(&self, src: &Path, rel: &str) -> Result<(), String> {
        let bytes = std::fs::read(src)
            .map_err(|e| format!("{} cannot be read to be copied in ({e})", src.display()))?;
        self.stage_bytes(&bytes, rel)
    }

    /// (F11-3) Write `bytes` into the worktree at the repo-relative path `rel` (a [`stageable_rel`]
    /// path) WITHOUT ever writing outside the worktree: every directory on the way must be a real
    /// directory inside the worktree - no component may be a symlink, a junction or any other
    /// reparse point (the checked-out revision may carry one) - and the destination must be absent
    /// or a regular file (never a link, a directory or anything else). The bytes go into a NEW file
    /// beside the destination (created exclusively under a random name, so nothing can be planted
    /// there), which then replaces the destination by a rename - the destination is never opened
    /// for writing, so a link or a hard link there cannot route the write elsewhere. The checks are
    /// repeated right before the rename. Any doubt is an `Err`: the caller refuses the run before
    /// the reviewer starts.
    pub fn stage_bytes(&self, bytes: &[u8], rel: &str) -> Result<(), String> {
        if !stageable_rel(rel) {
            return Err(format!("`{rel}` is not a path inside the repository"));
        }
        let dest = self.path.join(rel);
        self.real_parent_inside(&dest)?;
        dest_replaceable(&dest, rel)?;
        let parent = dest
            .parent()
            .ok_or_else(|| format!("`{rel}` has no parent"))?;
        let tmp = parent.join(format!(".c3-stage-{}.tmp", uuid::Uuid::new_v4().simple()));
        let written = (|| -> std::io::Result<()> {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()
        })();
        if let Err(e) = written {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("`{rel}` could not be written ({e})"));
        }
        // the same checks once more, right before the destination is replaced
        let again = self
            .parent_still_inside(parent)
            .and_then(|_| dest_replaceable(&dest, rel));
        if let Err(why) = again {
            let _ = std::fs::remove_file(&tmp);
            return Err(why);
        }
        std::fs::rename(&tmp, &dest).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("`{rel}` could not be put in place ({e})")
        })
    }

    /// Whether `parent` (inside the worktree) is still a real directory - not a link - whose
    /// canonical path lies inside the worktree's root.
    fn parent_still_inside(&self, parent: &Path) -> Result<(), String> {
        let root =
            real_dir_canonical(&self.path).map_err(|why| format!("the worktree root {why}"))?;
        let canon =
            real_dir_canonical(parent).map_err(|why| format!("{} {why}", parent.display()))?;
        if !canon.starts_with(&root) {
            return Err(format!(
                "{} resolves outside the worktree",
                parent.display()
            ));
        }
        Ok(())
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

/// (F11-3) A staging destination may be replaced only when it is absent or a regular file that is
/// not a link (read without following it); a link, a directory, anything else, or an entry that
/// cannot be read is refused.
fn dest_replaceable(dest: &Path, rel: &str) -> Result<(), String> {
    match std::fs::symlink_metadata(dest) {
        Ok(m) if is_link(&m) => Err(format!(
            "`{rel}` is a link in the worktree; nothing is copied through it"
        )),
        Ok(m) if m.is_file() => Ok(()),
        Ok(_) => Err(format!(
            "`{rel}` is not a regular file in the worktree; nothing is copied over it"
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("`{rel}` cannot be checked in the worktree ({e})")),
    }
}

/// Whether an entry's metadata (read WITHOUT following it) is a link: on Windows every reparse
/// point (`FILE_ATTRIBUTE_REPARSE_POINT` - a symlink, a junction, a mount point or any other
/// kind), the same test `panel/roles.rs` uses; elsewhere a symlink. (`setup::branches` reads a
/// throwaway path with it too: such a path is reported and never removed.)
pub(crate) fn is_link(meta: &std::fs::Metadata) -> bool {
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

/// (F11-4) The repo-relative brief reference with `.` and `..` components resolved, when the result
/// still lies inside the repository AND names the same file as `brief_path` (compared canonically:
/// on a system that resolves `..` after following a link, `sub/../b.md` can be another file than
/// `b.md`). Otherwise `rel` unchanged - a path that leaves the repository stays unstageable and is
/// refused by [`isolated_brief_refusal`]. An absolute reference is returned as it is.
pub fn normalized_brief_ref(repo: &Path, brief_path: &Path, rel: &str) -> String {
    if rel.is_empty() || Path::new(rel).is_absolute() || stageable_rel(rel) {
        return rel.to_string();
    }
    let mut parts: Vec<&str> = Vec::new();
    for c in rel.split(['/', '\\']) {
        match c {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return rel.to_string();
                }
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        return rel.to_string();
    }
    let norm = parts.join("/");
    let same = std::fs::canonicalize(brief_path)
        .ok()
        .zip(std::fs::canonicalize(repo.join(&norm)).ok())
        .is_some_and(|(a, b)| a == b);
    if same {
        norm
    } else {
        rel.to_string()
    }
}

/// (F11-3, F11-4) One entry of a revision's tree (`git ls-tree`): its mode and path.
struct TreeEntry {
    mode: String,
    path: String,
}

/// The entries `git ls-tree <rev> -- <paths>` lists (literal pathspecs; a path that is not in the
/// revision is simply absent). `Err` when git cannot read the revision's tree.
fn rev_tree_entries(repo: &Path, rev: &str, paths: &[String]) -> Result<Vec<TreeEntry>, String> {
    let mut args: Vec<&str> = vec!["--literal-pathspecs", "ls-tree", "-z", rev, "--"];
    args.extend(paths.iter().map(|p| p.as_str()));
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(&args)
        .output()
        .map_err(|e| format!("git ls-tree: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git ls-tree failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .split('\0')
        .filter_map(|rec| {
            let (meta, path) = rec.split_once('\t')?;
            let mode = meta.split_whitespace().next()?.to_string();
            Some(TreeEntry {
                mode,
                path: path.to_string(),
            })
        })
        .collect())
}

/// What a tree entry's mode is, in words.
fn mode_kind(mode: &str) -> &'static str {
    match mode {
        "040000" => "a directory",
        "120000" => "a link",
        "160000" => "a submodule",
        "100644" | "100755" => "a file",
        _ => "an entry of an unknown kind",
    }
}

/// (F11-3) Why the staging destination `rel` cannot be written safely in a worktree of `rev`, read
/// from the revision's own tree (so it holds whatever the platform's checkout makes of a link):
/// every directory on the way must be absent or a directory there - never a link, a submodule or a
/// file - and `rel` itself absent or a regular file. `None` when staging may proceed; a tree git
/// cannot read is a doubt, so it is a reason too.
pub fn rev_destination_problem(repo: &Path, rev: &str, rel: &str) -> Option<String> {
    let rel = rel.replace('\\', "/");
    let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return Some(format!("`{rel}` names no file"));
    }
    let prefixes: Vec<String> = (1..=parts.len()).map(|i| parts[..i].join("/")).collect();
    let entries = match rev_tree_entries(repo, rev, &prefixes) {
        Ok(e) => e,
        Err(why) => return Some(format!("the revision's tree could not be read ({why})")),
    };
    for (i, p) in prefixes.iter().enumerate() {
        let Some(e) = entries.iter().find(|e| &e.path == p) else {
            continue;
        };
        let last = i + 1 == prefixes.len();
        let ok = if last {
            matches!(e.mode.as_str(), "100644" | "100755")
        } else {
            e.mode == "040000"
        };
        if !ok {
            return Some(format!(
                "`{p}` is {} in the checked-out revision",
                mode_kind(&e.mode)
            ));
        }
    }
    None
}

/// (F11-4) The bytes of `rel` as a regular file committed at `rev` in `repo` (`git cat-file blob`),
/// `None` when the revision holds no regular file there.
pub fn committed_blob(repo: &Path, rev: &str, rel: &str) -> Option<Vec<u8>> {
    let rel = rel.replace('\\', "/");
    let entries = rev_tree_entries(repo, rev, std::slice::from_ref(&rel)).ok()?;
    if !entries
        .iter()
        .any(|e| e.path == rel && matches!(e.mode.as_str(), "100644" | "100755"))
    {
        return None;
    }
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["cat-file", "blob", &format!("{rev}:{rel}")])
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

/// (F11-4) The bytes of the brief as the reviewer's worktree gets them: the live file when it can
/// be read, else the regular file committed at the live checkout's `HEAD` (materialised from HEAD),
/// else the one committed at `rev` (the worktree's own revision). `Err` names why none is there.
pub fn brief_bytes(
    repo: &Path,
    brief_ref: &str,
    brief_path: Option<&Path>,
    rev: &str,
) -> Result<Vec<u8>, String> {
    let live = match brief_path {
        Some(p) => std::fs::read(p).map_err(|e| e.to_string()),
        None => Err("no brief file was resolved".to_string()),
    };
    let live_err = match live {
        Ok(b) => return Ok(b),
        Err(e) => e,
    };
    if let Some(b) = committed_blob(repo, "HEAD", brief_ref) {
        return Ok(b);
    }
    if rev != "HEAD" {
        if let Some(b) = committed_blob(repo, rev, brief_ref) {
            return Ok(b);
        }
    }
    Err(format!(
        "it cannot be read ({live_err}) and no regular file of that path is committed at HEAD"
    ))
}

/// (A1) The preflight of a seat whose reviewer runs in a throwaway worktree of `rev` (`HEAD`, or
/// the full sha of `--rev`): the prompt tells the reviewer to read the brief at `brief_ref`,
/// relative to its working directory - the worktree. The brief is staged there at that path
/// (a [`stageable_rel`] reference - `..` already resolved by [`normalized_brief_ref`]): from the
/// live file when it can be read, else (F11-4) materialised from the regular file committed at HEAD
/// or at `rev` ([`brief_bytes`]); an absolute reference resolves against the real file. (F11-3)
/// Staging also needs a destination that is safe to write in the revision's tree
/// ([`rev_destination_problem`]: no link, submodule or file on the way, no directory or link at the
/// path). Otherwise the reviewer would answer without its brief (xelixir panel 47cf7308,
/// 2026-11-12: "the required framing brief is missing") or the staging could reach outside the
/// worktree, so the seat is refused before anything starts. The refusal text carries
/// " is not usable:" - the wording the panel's re-seat classifies as a preflight-class refusal
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
    let why = if !stageable_rel(brief_ref) {
        "its path leaves the repository, so it cannot be copied in at the path the prompt names"
            .to_string()
    } else if let Some(p) = rev_destination_problem(repo, rev, brief_ref) {
        return Some(staging_refusal(
            engine,
            &format!("the brief `{brief_ref}`"),
            rev_label,
            &p,
        ));
    } else {
        match brief_bytes(repo, brief_ref, brief_path, rev) {
            Ok(_) => return None,
            Err(e) => e,
        }
    };
    Some(format!(
        "the {engine} seat is not usable: the brief `{brief_ref}` is neither committed at {rev_label} nor stageable into the reviewer's throwaway worktree of {rev_label} - {why}; nothing was started (commit the brief, place it inside the repository, or seat a reviewer that reads the live tree)"
    ))
}

/// (F11-3, F11-4) The typed pre-start refusal of a seat whose staging into its throwaway worktree
/// cannot be made: preflight-class wording (" is not usable:", `panel::run::is_preflight_refusal`),
/// so a panel re-seats an optional seat from its reserve exactly as for a preflight refusal.
pub fn staging_refusal(engine: &str, what: &str, rev_label: &str, why: &str) -> String {
    format!(
        "the {engine} seat is not usable: {what} cannot be staged into the reviewer's throwaway worktree of {rev_label} - {why}; nothing was started (commit the brief, place it inside the repository, or seat a reviewer that reads the live tree)"
    )
}

/// (A1) How many uncommitted paths the prompt of a worktree-isolated reviewer names.
pub const UNCOMMITTED_PATHS_SHOWN: usize = 20;

/// (A1, F11-5) What the live tree held uncommitted at launch, as the prompt of a reviewer in a
/// throwaway worktree of HEAD may tell it: the TRACKED paths of HEAD whose live content differs
/// (modified, deleted, type-changed; a rename or copy by its ORIGINAL path) - names the reviewer
/// can see in the revision anyway - and only the COUNT of the paths HEAD does not hold (untracked,
/// or newly added to the index), whose names never reach the prompt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Uncommitted {
    /// The tracked paths of HEAD that differ in the live tree, ordinal-sorted.
    pub tracked: Vec<String>,
    /// How many paths HEAD does not hold (an untracked directory counts once, as git shows it).
    pub new_paths: usize,
}

impl Uncommitted {
    pub fn is_empty(&self) -> bool {
        self.tracked.is_empty() && self.new_paths == 0
    }
}

/// (A1, F11-5) The live tree's uncommitted state at launch (`git status --porcelain=v1 -z`):
/// [`Uncommitted`]. Empty when the tree is clean or git cannot be run.
pub fn uncommitted_paths(repo: &Path) -> Uncommitted {
    let out = match std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["--no-optional-locks", "status", "--porcelain=v1", "-z"])
        .output()
    {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Uncommitted::default(),
    };
    let text = String::from_utf8_lossy(&out);
    let tokens: Vec<&str> = text.split('\0').collect();
    let mut u = Uncommitted::default();
    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i];
        i += 1;
        if tok.len() < 4 {
            continue;
        }
        let xy: Vec<char> = tok[0..2].chars().collect();
        let path = tok[3..].to_string();
        // a rename/copy entry is followed by its ORIGINAL path - the one HEAD holds.
        if xy.iter().any(|c| matches!(c, 'R' | 'C')) {
            if let Some(orig) = tokens.get(i) {
                i += 1;
                u.tracked.push(orig.to_string());
            }
            continue;
        }
        if xy[0] == '?' || xy[0] == '!' || xy[0] == 'A' {
            u.new_paths += 1;
        } else {
            u.tracked.push(path);
        }
    }
    u.tracked.sort();
    u.tracked.dedup();
    u
}

/// (F11-5) A path as prompt DATA: shown between backticks only when it holds no backtick, no
/// control character and no line break (such a name could close the code span or start a new
/// instruction line); `None` when it must be counted instead.
fn prompt_safe_path(p: &str) -> Option<String> {
    if p.is_empty() || p.chars().any(|c| c == '`' || c.is_control()) {
        None
    } else {
        Some(format!("`{p}`"))
    }
}

/// (A1, F11-5) The prompt paragraph that tells a reviewer in a throwaway worktree of HEAD what it
/// cannot see: the tracked paths of HEAD that differ in the live checkout at launch (at most
/// [`UNCOMMITTED_PATHS_SHOWN`] named, each as data - [`prompt_safe_path`] - the rest counted), the
/// COUNT of paths HEAD does not hold (never their names: an untracked file's name can be
/// confidential and is outside the revision under review), and that the brief was copied in.
/// `""` when the tree was clean (nothing is missing).
pub fn isolation_prompt_line(
    head_short: &str,
    uncommitted: &Uncommitted,
    brief_ref: &str,
) -> String {
    if uncommitted.is_empty() {
        return String::new();
    }
    let mut l = format!(
        "Isolation: your working directory is a throwaway git worktree of the committed HEAD {head_short}, not the live checkout."
    );
    if !uncommitted.tracked.is_empty() {
        let mut shown: Vec<String> = Vec::new();
        let mut withheld = 0usize;
        for p in &uncommitted.tracked {
            match prompt_safe_path(p) {
                Some(s) if shown.len() < UNCOMMITTED_PATHS_SHOWN => shown.push(s),
                _ => withheld += 1,
            }
        }
        l.push_str(&format!(
            " {} tracked path(s) had uncommitted changes in the live checkout at launch, so your worktree holds their committed version",
            uncommitted.tracked.len()
        ));
        if !shown.is_empty() {
            l.push_str(&format!(": {}", shown.join(", ")));
        }
        if withheld > 0 {
            l.push_str(&format!(" (+{withheld} more)"));
        }
        l.push('.');
    }
    if uncommitted.new_paths > 0 {
        l.push_str(&format!(
            " {} path(s) that are not in HEAD (untracked or newly added) existed only in the live checkout and are not in your worktree.",
            uncommitted.new_paths
        ));
    }
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

    /// An injected owner-record failure (a full disk, a denied write).
    fn failing_owner_record(_worktree: &Path) -> Result<(), String> {
        Err("injected owner-record failure".to_string())
    }

    // (R2, F13-8) The owner record is not best effort: when it cannot be written, creating the
    // throwaway worktree fails (the capability failure of a worktree that could not be made) and
    // the worktree made so far is removed; the real recorder writes a record naming this process.
    #[test]
    fn a_worktree_whose_owner_record_cannot_be_written_is_removed_and_not_used() {
        let repo = brief_repo("owner-fails");
        let err = match ThrowawayWorktree::create_with(&repo, "HEAD", failing_owner_record) {
            Ok(wt) => panic!("created without an owner record: {}", wt.path().display()),
            Err(e) => e,
        };
        assert!(
            err.contains("its owner record could not be written (injected owner-record failure)"),
            "{err}"
        );
        assert!(err.contains("the worktree was removed"), "{err}");
        let list = git_capture(&repo, &["worktree", "list", "--porcelain"]).unwrap();
        assert!(!list.contains(THROWAWAY_WORKTREE_PREFIX), "{list}");
        // the real recorder: `<pid> <start>` of this process in the admin directory
        let wt = ThrowawayWorktree::create(&repo, "HEAD").unwrap();
        let dotgit = std::fs::read_to_string(wt.path().join(".git")).unwrap();
        let admin = dotgit
            .trim()
            .strip_prefix("gitdir:")
            .unwrap()
            .trim()
            .to_string();
        let rec = std::fs::read_to_string(Path::new(&admin).join("c3-owner")).unwrap();
        assert_eq!(
            crate::setup::branches::parse_owner_record(&rec).map(|(p, _)| p),
            Some(std::process::id()),
            "{rec}"
        );
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

    /// A file link at `link` to `target` (a file symlink): `false` when the system refuses to make
    /// one (on Windows a file symlink needs a privilege or developer mode).
    fn try_file_link(link: &Path, target: &Path) -> bool {
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(target, link).is_ok()
        }
        #[cfg(not(windows))]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
    }

    // (F11-3) staging never writes through a link in the worktree: a directory component that is a
    // link (a junction on Windows, a symlink elsewhere) and a destination that is a file link are
    // refused, the outside sentinel keeps its bytes; a destination that is a directory is refused;
    // a regular file is replaced (by a rename, never written in place).
    #[test]
    fn staging_never_writes_through_a_link_in_the_worktree() {
        let repo = brief_repo("stage-links");
        let outside = repo.parent().unwrap().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("sentinel.txt"), "keep me\n").unwrap();
        std::fs::write(outside.join("brief.md"), "outside brief\n").unwrap();
        let wt = ThrowawayWorktree::create(&repo, "HEAD").unwrap();
        // a directory component that is a link
        make_dir_link(&wt.path().join("docs"), &outside);
        let why = wt
            .stage_bytes(b"the operator's brief\n", "docs/brief.md")
            .expect_err("a linked directory component is refused");
        assert!(why.contains("is a link in the worktree"), "{why}");
        assert_eq!(
            std::fs::read_to_string(outside.join("brief.md")).unwrap(),
            "outside brief\n",
            "nothing was written through the junction"
        );
        // a destination that is a file link
        let flink = wt.path().join("linked.md");
        if try_file_link(&flink, &outside.join("sentinel.txt")) {
            let why = wt
                .stage_bytes(b"x\n", "linked.md")
                .expect_err("a file link destination is refused");
            assert!(why.contains("is a link in the worktree"), "{why}");
        } else {
            eprintln!(
                "skipped the file-link half of staging_never_writes_through_a_link_in_the_worktree: this system refused to create a file symlink (it needs a privilege or developer mode on Windows)"
            );
        }
        assert_eq!(
            std::fs::read_to_string(outside.join("sentinel.txt")).unwrap(),
            "keep me\n"
        );
        // a destination that is a directory
        std::fs::create_dir_all(wt.path().join("adir")).unwrap();
        let why = wt.stage_bytes(b"x\n", "adir").unwrap_err();
        assert!(why.contains("not a regular file"), "{why}");
        // a regular file is replaced, and no staging temp is left behind
        wt.stage_bytes(b"replaced\n", "committed.md").unwrap();
        assert_eq!(
            std::fs::read_to_string(wt.path().join("committed.md")).unwrap(),
            "replaced\n"
        );
        assert!(!std::fs::read_dir(wt.path())
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with(".c3-stage-")));
        drop(wt);
        assert_eq!(
            std::fs::read_to_string(outside.join("sentinel.txt")).unwrap(),
            "keep me\n"
        );
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }

    /// Commit, in `repo`, a LINK at `rel` whose target is `target` (mode 120000 through the index -
    /// no privilege needed on any platform), while the live tree keeps a REGULAR file at `rel` with
    /// `live` (the reviewer's RC1 layout).
    fn commit_link_entry(repo: &Path, rel: &str, target: &Path, live: &str) {
        let target_s = target.to_string_lossy().replace('\\', "/");
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["hash-object", "-w", "--stdin"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut c| {
                use std::io::Write;
                c.stdin.take().unwrap().write_all(target_s.as_bytes())?;
                c.wait_with_output()
            })
            .unwrap();
        let blob = String::from_utf8_lossy(&out.stdout).trim().to_string();
        git_capture(
            repo,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("120000,{blob},{rel}"),
            ],
        )
        .unwrap();
        git_capture(repo, &["commit", "-q", "-m", "a link at the brief's path"]).unwrap();
        let live_path = repo.join(rel);
        if let Ok(m) = std::fs::symlink_metadata(&live_path) {
            if is_link(&m) {
                let _ = remove_link(&live_path, &m);
            }
        }
        if let Some(d) = live_path.parent() {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(&live_path, live).unwrap();
    }

    // (F11-3, the reviewer's RC1) HEAD carries a LINK at the brief's path pointing at a sentinel
    // outside, the live tree a regular brief there: the seat is refused before anything starts
    // (preflight-class wording, so a panel re-seats it), and the staging itself refuses too - the
    // sentinel keeps its bytes.
    #[test]
    fn a_committed_link_at_the_brief_path_refuses_the_seat_and_never_writes_through_it() {
        let repo = brief_repo("rc1");
        let outside = repo.parent().unwrap().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let sentinel = outside.join("sentinel.txt");
        std::fs::write(&sentinel, "keep me\n").unwrap();
        commit_link_entry(&repo, "brief.md", &sentinel, "# the live brief\n");
        let head = git_capture(&repo, &["rev-parse", "HEAD"]).unwrap();
        let p = rev_destination_problem(&repo, &head, "brief.md").expect("a link is a problem");
        assert!(p.contains("`brief.md` is a link"), "{p}");
        let r = isolated_brief_refusal(
            &repo,
            "kimi",
            "brief.md",
            Some(&repo.join("brief.md")),
            "HEAD",
            "HEAD",
        )
        .expect("the seat is refused");
        assert!(r.starts_with("the kimi seat is not usable:"), "{r}");
        assert!(r.contains("nothing was started"), "{r}");
        assert!(crate::panel::run::is_preflight_refusal(&r), "{r}");
        // the end-to-end staging of the run refuses before writing anything
        let plan = crate::consult::reviewed::IsolationPlan {
            reason: crate::consult::reviewed::Reason::Engine,
            rev: "HEAD".into(),
            sha: head.clone(),
        };
        let collab = repo.join(".collab");
        let got = crate::consult::reviewed::setup_review_tree(
            &plan,
            &crate::consult::reviewed::StageInputs {
                repo_root: &repo,
                collab_root: &collab,
                task: "t",
                brief_ref: "brief.md",
                brief_path: Some(&repo.join("brief.md")),
                engine: "kimi",
            },
        );
        match got {
            Err(crate::consult::reviewed::SetupError::Refused(m)) => {
                assert!(crate::panel::run::is_preflight_refusal(&m), "{m}");
                assert!(m.contains("`brief.md` is a link"), "{m}");
            }
            Err(e) => panic!("expected a typed staging refusal, got {e:?}"),
            Ok(_) => panic!("expected a typed staging refusal, got a staged tree"),
        }
        // where the checkout made a real link of it (Unix; Windows with core.symlinks), the
        // filesystem check alone refuses it too
        let wt = ThrowawayWorktree::create(&repo, "HEAD").unwrap();
        let in_wt = wt.path().join("brief.md");
        if std::fs::symlink_metadata(&in_wt).is_ok_and(|m| is_link(&m)) {
            let why = wt.stage_bytes(b"live\n", "brief.md").unwrap_err();
            assert!(why.contains("is a link in the worktree"), "{why}");
        } else {
            eprintln!("the checkout made a plain file of the committed link (core.symlinks is off): only the revision-tree check applies");
        }
        drop(wt);
        assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "keep me\n");
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }

    /// Make every `git worktree add` of `repo` plant a directory link `docs` -> `target` in the new
    /// worktree (a post-checkout hook, through a repo-local `core.hooksPath`): what a checkout that
    /// carries a link leaves on disk, without needing the privilege a committed symlink needs on
    /// Windows. A junction on Windows (`mklink /J` needs no privilege), a symlink elsewhere.
    pub(crate) fn plant_dir_link_on_checkout(repo: &Path, target: &Path) {
        let hooks = repo.parent().unwrap().join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        #[cfg(windows)]
        let body = format!(
            "#!/bin/sh\ncmd //c \"mklink /J docs {}\" >/dev/null 2>&1\nexit 0\n",
            target.display()
        );
        #[cfg(not(windows))]
        let body = format!("#!/bin/sh\nln -s '{}' docs\nexit 0\n", target.display());
        let hook = hooks.join("post-checkout");
        std::fs::write(&hook, body).unwrap();
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        git_capture(
            repo,
            &["config", "core.hooksPath", &hooks.to_string_lossy()],
        )
        .unwrap();
    }

    // (F11-3) a link that appears in the worktree's checkout (here planted by a post-checkout hook)
    // on the way to the brief: the run's staging is a typed pre-start refusal, nothing is written
    // through the link.
    #[test]
    fn a_link_in_the_checkout_on_the_briefs_way_is_a_typed_staging_refusal() {
        let repo = brief_repo("hook-link");
        let outside = repo.parent().unwrap().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("brief.md"), "outside brief\n").unwrap();
        std::fs::create_dir_all(repo.join("docs")).unwrap();
        std::fs::write(repo.join("docs").join("brief.md"), "# live brief\n").unwrap();
        plant_dir_link_on_checkout(&repo, &outside);
        let head = git_capture(&repo, &["rev-parse", "HEAD"]).unwrap();
        // the revision itself is fine: the preflight lets the seat through
        assert!(isolated_brief_refusal(
            &repo,
            "kimi",
            "docs/brief.md",
            Some(&repo.join("docs").join("brief.md")),
            "HEAD",
            "HEAD"
        )
        .is_none());
        let plan = crate::consult::reviewed::IsolationPlan {
            reason: crate::consult::reviewed::Reason::Engine,
            rev: "HEAD".into(),
            sha: head,
        };
        let collab = repo.join(".collab");
        let got = crate::consult::reviewed::setup_review_tree(
            &plan,
            &crate::consult::reviewed::StageInputs {
                repo_root: &repo,
                collab_root: &collab,
                task: "t",
                brief_ref: "docs/brief.md",
                brief_path: Some(&repo.join("docs").join("brief.md")),
                engine: "kimi",
            },
        );
        match got {
            Err(crate::consult::reviewed::SetupError::Refused(m)) => {
                assert!(
                    m.starts_with(
                        "the kimi seat is not usable: the brief `docs/brief.md` cannot be staged"
                    ),
                    "{m}"
                );
                assert!(m.contains("is a link in the worktree"), "{m}");
                assert!(crate::panel::run::is_preflight_refusal(&m), "{m}");
            }
            Err(e) => panic!("expected a typed staging refusal, got {e:?}"),
            Ok(_) => panic!("the hook did not plant the link, or the staging went through it"),
        }
        assert_eq!(
            std::fs::read_to_string(outside.join("brief.md")).unwrap(),
            "outside brief\n"
        );
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }

    // (F11-4) an in-repo reference with `..` is normalised to the same file; one that leaves the
    // repository is left as it is (and stays unstageable).
    #[test]
    fn an_in_repo_brief_path_with_dot_dot_is_normalised() {
        let repo = brief_repo("dotdot");
        std::fs::create_dir_all(repo.join("sub")).unwrap();
        let bp = repo.join("sub").join("..").join("committed.md");
        assert_eq!(
            normalized_brief_ref(&repo, &bp, "sub/../committed.md"),
            "committed.md"
        );
        assert_eq!(
            normalized_brief_ref(&repo, &bp, "./sub/./../committed.md"),
            "committed.md"
        );
        assert_eq!(
            normalized_brief_ref(&repo, &bp, "../outside.md"),
            "../outside.md"
        );
        assert_eq!(normalized_brief_ref(&repo, &bp, "docs/b.md"), "docs/b.md");
        // a normalised path that names ANOTHER file than the one resolved is not taken
        std::fs::write(repo.join("other.md"), "o\n").unwrap();
        assert_eq!(
            normalized_brief_ref(&repo, &repo.join("other.md"), "sub/../committed.md"),
            "sub/../committed.md"
        );
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }

    // (F11-4) a brief committed at HEAD but unreadable in the live tree is materialised from HEAD
    // (also into a worktree of an older revision that does not hold it); the revision-tree check
    // refuses a destination that is a directory or lies under a file.
    #[test]
    fn a_committed_brief_unreadable_live_is_materialised_from_head() {
        let repo = brief_repo("materialise");
        let first = git_capture(&repo, &["rev-parse", "HEAD"]).unwrap();
        std::fs::create_dir_all(repo.join("docs")).unwrap();
        std::fs::write(repo.join("docs").join("brief.md"), "# committed brief\n").unwrap();
        git_capture(&repo, &["add", "-A"]).unwrap();
        git_capture(&repo, &["commit", "-q", "-m", "brief"]).unwrap();
        let missing = repo.join("docs").join("gone.md");
        // the live source cannot be read: HEAD's bytes
        let b = brief_bytes(&repo, "docs/brief.md", Some(&missing), &first).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&b).replace("\r\n", "\n"),
            "# committed brief\n"
        );
        assert!(isolated_brief_refusal(
            &repo,
            "kimi",
            "docs/brief.md",
            Some(&missing),
            &first,
            "revision x"
        )
        .is_none());
        let plan = crate::consult::reviewed::IsolationPlan {
            reason: crate::consult::reviewed::Reason::Rev,
            rev: first[..9].to_string(),
            sha: first.clone(),
        };
        let collab = repo.join(".collab");
        let tree = crate::consult::reviewed::setup_review_tree(
            &plan,
            &crate::consult::reviewed::StageInputs {
                repo_root: &repo,
                collab_root: &collab,
                task: "t",
                brief_ref: "docs/brief.md",
                brief_path: Some(&missing),
                engine: "codex",
            },
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(
            std::fs::read_to_string(tree.path().join("docs").join("brief.md"))
                .unwrap()
                .replace("\r\n", "\n"),
            "# committed brief\n",
            "the worktree of the older revision got HEAD's brief"
        );
        drop(tree);
        // neither live nor committed: refused
        let r = brief_bytes(&repo, "docs/none.md", Some(&missing), "HEAD").unwrap_err();
        assert!(
            r.contains("no regular file of that path is committed at HEAD"),
            "{r}"
        );
        // the destination is a directory at HEAD, or lies under a file
        let head = git_capture(&repo, &["rev-parse", "HEAD"]).unwrap();
        let d = rev_destination_problem(&repo, &head, "docs").unwrap();
        assert!(d.contains("`docs` is a directory"), "{d}");
        let f = rev_destination_problem(&repo, &head, "committed.md/x.md").unwrap();
        assert!(f.contains("`committed.md` is a file"), "{f}");
        assert!(rev_destination_problem(&repo, &head, "docs/brief.md").is_none());
        assert!(rev_destination_problem(&repo, &head, "new/dir/x.md").is_none());
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }

    // (A1, F11-5) the reviewer is told the tracked paths that differ (at most 20 named, then the
    // count) and only the COUNT of the paths HEAD does not hold; a name that could break out of
    // its code span is counted, never shown.
    #[test]
    fn the_isolation_prompt_names_tracked_changes_and_counts_the_rest() {
        assert_eq!(
            isolation_prompt_line("abc", &Uncommitted::default(), "b.md"),
            ""
        );
        let tracked: Vec<String> = (0..25).map(|i| format!("src/f{i:02}.rs")).collect();
        let u = Uncommitted {
            tracked,
            new_paths: 3,
        };
        let l = isolation_prompt_line("abc123", &u, ".collab/t/h/01.md");
        assert!(l.starts_with("Isolation: your working directory is a throwaway git worktree of the committed HEAD abc123"), "{l}");
        assert!(
            l.contains("25 tracked path(s) had uncommitted changes"),
            "{l}"
        );
        assert!(
            l.contains("`src/f19.rs`") && !l.contains("`src/f20.rs`"),
            "{l}"
        );
        assert!(l.contains("(+5 more)."), "{l}");
        assert!(l.contains(" 3 path(s) that are not in HEAD"), "{l}");
        assert!(l.ends_with(
            "The brief `.collab/t/h/01.md` was copied into your worktree at that path."
        ));
        // names with a backtick, a line break or a control character are counted, not shown
        let odd = Uncommitted {
            tracked: vec![
                "ok.rs".into(),
                "a`b.rs".into(),
                "line\nbreak.rs".into(),
                "tab\tx.rs".into(),
            ],
            new_paths: 0,
        };
        let l = isolation_prompt_line("abc", &odd, "");
        assert!(l.contains(": `ok.rs` (+3 more)."), "{l}");
        assert!(
            !l.contains("a`b") && !l.contains("break.rs") && !l.contains("tab"),
            "{l}"
        );
        // from a live repo (the reviewer's Verify): a modified tracked file is named, a
        // confidential untracked file is only counted.
        let repo = brief_repo("iso");
        assert_eq!(isolation_prompt(&repo, ""), "");
        std::fs::write(repo.join("committed.md"), "# changed\n").unwrap();
        std::fs::write(repo.join("secret-acquisition-plan.txt"), "n\n").unwrap();
        assert_eq!(
            uncommitted_paths(&repo),
            Uncommitted {
                tracked: vec!["committed.md".to_string()],
                new_paths: 1
            }
        );
        let live = isolation_prompt(&repo, "");
        assert!(live.contains(": `committed.md`."), "{live}");
        assert!(live.contains(" 1 path(s) that are not in HEAD"), "{live}");
        assert!(!live.contains("secret-acquisition-plan"), "{live}");
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
