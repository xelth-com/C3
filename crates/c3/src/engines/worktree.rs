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
//!
//! (0.2.5 D7) A run that resumes a thread of a worktree-isolated engine makes its worktree at the
//! path of the run it resumes ([`ThrowawayWorktree::create_at`]: only a path of C3's own shape,
//! [`reusable_worktree_path`], and only when it is free - the engine binds its session to that
//! directory). (0.2.5 A4, F57-7) That path is BUILT from the temp directory's handle and the recorded
//! name (the recorded parent must be the temp directory by file identity), and every worktree's
//! directory is claimed through that handle and kept by its identity through the checkout
//! ([`ThrowawayWorktree::create_at`]).

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::contain::{self, Kind, Root, Step, StepError};

/// The directory-name prefix of a reviewer's throwaway worktree (directly under the system temp
/// directory, followed by the run's 32-hex uuid). `setup::branches` leaves such a worktree out of its
/// "nobody owns this" count while a live run owns it, and reports it as stale once none does. The
/// name dates from the kimi engine, the first user; it is kept so the recognition of a leftover made
/// by an older build does not change.
pub const THROWAWAY_WORKTREE_PREFIX: &str = "c3-kimi-wt-";

/// (0.2.5 R5, F18-2 / F20-2) A git command run in `repo` that takes no OPTIONAL lock
/// (`GIT_OPTIONAL_LOCKS=0`, the environment form of `--no-optional-locks`): setting up or checking a
/// review worktree never refreshes, rewrites or locks the live repository's index (a parallel
/// session's `git add` is never refused for a review's `index.lock`).
pub(crate) fn git_cmd(repo: &Path) -> std::process::Command {
    let mut c = std::process::Command::new("git");
    c.arg("-C").arg(repo).env("GIT_OPTIONAL_LOCKS", "0");
    c
}

/// Run a git subcommand in `repo` ([`git_cmd`]: no optional lock), returning trimmed stdout or an
/// error.
pub(crate) fn git_capture(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = git_cmd(repo)
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

/// The full sha of the commit `rev` names in `repo` (`git rev-parse --verify --end-of-options
/// <rev>^{commit}`). A revision that starts with `-` or holds a blank or a control character is
/// refused; (R5, F19-5) and the value is passed after `--end-of-options` anyway, so git never reads
/// it as an option.
pub fn resolve_commit(repo: &Path, rev: &str) -> Result<String, String> {
    let rev = rev.trim();
    if rev.is_empty()
        || rev.starts_with('-')
        || rev.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(format!("'{}' is not a revision", rev.escape_debug()));
    }
    let sha = git_capture(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
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
    /// (R5, F18-6) The worktree-relative paths of the links the checkout left whose target lies
    /// outside the worktree (or cannot be resolved), removed right after the checkout.
    outside_links: Vec<String>,
    /// The outcome of the one removal attempt (`None`: not tried yet).
    removal: Option<Result<(), String>>,
    /// (0.2.5 D4) The worktree's directory, opened ONCE right after the checkout without following
    /// it and verified on that handle ([`Root::open`]): the post-checkout sweep, every staging write
    /// and the removal work relative to it, never by the path name. `None` only for a leftover that
    /// `c3 doctor --fix` removes ([`remove_throwaway_worktree`] opens it then).
    root: Option<Root>,
}

/// (0.2.5 D7) Why a throwaway worktree was not made ([`ThrowawayWorktree::create_at`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateError {
    /// The path it was asked for is taken: something (a directory, a file, a link) is there - a
    /// live run of the same thread, or a crashed run's leftover. Nothing was made, nothing was
    /// removed.
    Occupied(PathBuf),
    /// The worktree could not be made (the words say why).
    Failed(String),
    /// (0.2.5 A4, F57-7) The directory this call claimed at the path is not the one there any more
    /// (the words say what was found): it was swapped between the claim and the check after the
    /// checkout. The run is refused; NOTHING at the path is removed (whatever sits there is not
    /// this run's), and the registration `git worktree add` made is pruned.
    Replaced(PathBuf, String),
}

impl CreateError {
    /// The failure in words.
    pub fn words(&self) -> String {
        match self {
            CreateError::Occupied(p) => format!("the path {} is taken", p.display()),
            CreateError::Failed(why) => why.clone(),
            CreateError::Replaced(p, why) => format!(
                "the directory made for the worktree at {} is not the one at that path any more ({why}); nothing there was removed",
                p.display()
            ),
        }
    }
}

/// (0.2.5 A4, F57-7) A directory's identity read on an open handle - the same (volume / device,
/// file index / inode) pair as [`contain::FileId`].
fn dir_id(d: &cap_std::fs::Dir) -> std::io::Result<contain::FileId> {
    let m = d.dir_metadata()?;
    Ok((
        cap_fs_ext::MetadataExt::dev(&m),
        cap_fs_ext::MetadataExt::ino(&m),
    ))
}

/// (0.2.5 A4, F57-7) The CURRENT system temp directory ([`std::env::temp_dir`]), opened once as a
/// directory handle: a throwaway worktree's path is built from it, its directory is claimed through
/// it, and a recorded path's parent is compared with it by identity.
fn open_temp_dir() -> Result<Root, String> {
    let t = std::env::temp_dir();
    Root::open_anchor(&t)
        .map_err(|e| format!("the system temp directory {} {}", t.display(), e.words()))
}

/// (0.2.5 D7) Whether `recorded` - the throwaway worktree path a run recorded - has the shape C3
/// gives one, so a run that RESUMES that run's thread may make its own worktree at the same path (a
/// worktree-isolated engine binds its session to the directory it was made in): an absolute path
/// without a `.` or `..` component, directly under the CURRENT system temp directory and named
/// [`THROWAWAY_WORKTREE_PREFIX`] followed by 32 lower-case hex digits
/// ([`crate::setup::branches::is_throwaway_name`]). `Err` says why it is not reusable. Whether the
/// path is FREE is decided when the worktree is made ([`ThrowawayWorktree::create_at`]).
///
/// (0.2.5 A4, F57-7) The path is BUILT, not reused: `Ok` is the temp directory joined with the
/// validated name. "Directly under the temp directory" is decided by NATIVE identity - the recorded
/// parent and the temp directory, each opened as a directory, must have the same file identity
/// (volume and file index on Windows, device and inode elsewhere) - never by comparing path text,
/// and no separator is rewritten (a literal backslash in a Unix name stays part of that name).
pub fn reusable_worktree_path(recorded: &str) -> Result<PathBuf, String> {
    let temp = open_temp_dir()?;
    reusable_under(recorded, &temp)
}

/// [`reusable_worktree_path`] against the opened temp directory `temp`.
fn reusable_under(recorded: &str, temp: &Root) -> Result<PathBuf, String> {
    if recorded.is_empty() {
        return Err("no path was recorded".into());
    }
    let p = PathBuf::from(recorded);
    if !p.is_absolute() {
        return Err("it is not an absolute path".into());
    }
    if p.components().any(|c| {
        matches!(
            c,
            std::path::Component::CurDir | std::path::Component::ParentDir
        )
    }) {
        return Err("it holds a `.` or `..` component".into());
    }
    let Some(name) = p
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| crate::setup::branches::is_throwaway_name(n))
    else {
        return Err(format!(
            "its name is not `{THROWAWAY_WORKTREE_PREFIX}` followed by 32 lower-case hex digits"
        ));
    };
    // the recorded parent, opened as a directory, must BE the temp directory (file identity)
    let under_temp = p
        .parent()
        .and_then(|d| Root::open_anchor(d).ok())
        .is_some_and(|d| d.id() == temp.id());
    if !under_temp {
        return Err("it is not directly under the system temp directory".into());
    }
    Ok(temp.path().join(name))
}

/// (0.2.5 D7) The pre-start refusal of a resuming seat whose worktree path is taken: one line that
/// names the path and what to do, in preflight-class wording (" is not usable:"), so a panel
/// re-seats an optional seat from its reserve exactly as for a preflight refusal.
pub fn occupied_refusal(engine: &str, path: &Path) -> String {
    format!(
        "the {engine} seat is not usable: the worktree path {} of the thread it resumes is taken (a live run of that thread, or a crashed run's leftover); nothing was started - a live run must finish first, and `c3 doctor --fix` removes a crashed run's leftover",
        path.display()
    )
}

/// (0.2.5 A4, F57-7) The pre-start refusal of a seat whose claimed worktree directory was replaced
/// before the checkout was verified ([`CreateError::Replaced`]): one line in preflight-class wording
/// (" is not usable:").
pub fn replaced_refusal(engine: &str, path: &Path, why: &str) -> String {
    format!(
        "the {engine} seat is not usable: the directory made for its throwaway worktree at {} is not the one at that path any more ({why}); nothing was started and nothing there was removed",
        path.display()
    )
}

/// (0.2.5 A4, F57-7) The directory a creation made for its worktree: the handle it was opened by
/// right after the exclusive create (through the temp directory's handle, without following it),
/// kept open so the identity is always read from the directory itself.
struct Claim {
    dir: cap_std::fs::Dir,
}

impl Claim {
    /// Whether the directory `other` is the claimed one (identities read on both handles; one that
    /// cannot be read is not the same).
    fn is(&self, other: &cap_std::fs::Dir) -> bool {
        matches!((dir_id(&self.dir), dir_id(other)), (Ok(a), Ok(b)) if a == b)
    }
}

/// (0.2.5 A4, F57-7) Claim the worktree directory `name` in the temp directory `temp` (`path` names
/// it in messages; `given`: the path was asked for, a resume): made EXCLUSIVELY through `temp`'s
/// handle - an existing entry is [`CreateError::Occupied`] for a given path and never touched - then
/// opened through the same handle WITHOUT following it and kept. What was made a moment ago is an
/// empty real directory: a link, anything else, or a directory with an entry in it means another
/// directory took the name in between ([`CreateError::Replaced`]; nothing is removed).
fn claim_dir(temp: &Root, name: &OsStr, path: &Path, given: bool) -> Result<Claim, CreateError> {
    match temp.dir().create_dir(name) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && given => {
            return Err(CreateError::Occupied(path.to_path_buf()));
        }
        Err(e) => {
            return Err(CreateError::Failed(format!(
                "its directory {} could not be made ({e})",
                path.display()
            )));
        }
    }
    interleave("worktree-claim");
    let replaced = |why: String| CreateError::Replaced(path.to_path_buf(), why);
    let dir = contain::open_child(temp.dir(), name, false)
        .map_err(|why| replaced(format!("the path {why}")))?;
    match contain::list(&dir) {
        Ok(names) if names.is_empty() => {}
        Ok(_) => return Err(replaced("the directory at the path is not empty".into())),
        Err(e) => return Err(replaced(format!("the directory cannot be listed ({e})"))),
    }
    dir_id(&dir).map_err(|e| replaced(format!("its identity cannot be read ({e})")))?;
    Ok(Claim { dir })
}

/// (0.2.5 A4, F57-7) After a failed `git worktree add`: remove the directory [`claim_dir`] made -
/// through the temp directory's handle, only while the name still opens (without following it) to
/// that same directory (its identity), and only when it is empty (a directory removal). Returns the
/// words a failure appends (`""` when it is gone).
fn remove_claimed(temp: &Root, name: &OsStr, path: &Path, claim: Claim) -> String {
    let again = match contain::open_child(temp.dir(), name, false) {
        Ok(d) => d,
        Err(Step::Missing) => return String::new(),
        Err(why) => {
            return format!(
                "; the path {} {why} now, so nothing there was removed",
                path.display()
            )
        }
    };
    if !claim.is(&again) {
        return format!(
            "; the path {} no longer names the directory made for it, so nothing there was removed",
            path.display()
        );
    }
    // (Windows) a held handle refuses the removal: both are released first
    drop(again);
    drop(claim);
    match temp.dir().remove_dir(name) {
        Ok(()) => String::new(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => format!(
            "; the directory {} made for it was left in place ({e})",
            path.display()
        ),
    }
}

/// (0.2.5 A4) A point where a test interleaves its own step: the race hook of [`contain`]
/// ([`contain::race_point`]) and, in a unit test, a step this thread registered for the point
/// ([`interleave_hook::at`]). Does nothing otherwise.
pub(crate) fn interleave(point: &str) {
    contain::race_point(point);
    #[cfg(test)]
    interleave_hook::run(point);
}

/// (0.2.5 A4) The unit tests' interleaving: a step registered for a named point runs ONCE, the first
/// time this thread reaches that point ([`interleave`]).
#[cfg(test)]
pub(crate) mod interleave_hook {
    use std::cell::RefCell;

    type Steps = Vec<(String, Box<dyn FnOnce()>)>;

    thread_local! {
        static STEPS: RefCell<Steps> = const { RefCell::new(Vec::new()) };
    }

    /// Clears this thread's registered steps when dropped.
    pub(crate) struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            STEPS.with(|s| s.borrow_mut().clear());
        }
    }

    /// Run `step` the first time this thread reaches `point`.
    pub(crate) fn at(point: &str, step: impl FnOnce() + 'static) -> Guard {
        STEPS.with(|s| s.borrow_mut().push((point.to_string(), Box::new(step))));
        Guard
    }

    /// Take the first step registered for `point` (if any) and run it - outside the borrow, so the
    /// step may reach a point itself.
    pub(crate) fn run(point: &str) {
        let step = STEPS.with(|s| {
            let mut s = s.borrow_mut();
            let i = s.iter().position(|(p, _)| p == point)?;
            Some(s.remove(i).1)
        });
        if let Some(step) = step {
            step();
        }
    }
}

/// How a failed creation's clean-up ended, in words.
fn removal_words(r: Result<(), String>) -> String {
    match r {
        Ok(()) => "the worktree was removed".to_string(),
        Err(e) => format!(
            "the worktree was left in place ({e}); `c3 doctor` reports it for removal by hand"
        ),
    }
}

impl ThrowawayWorktree {
    /// Make a fresh detached worktree of `rev` (a full sha, `HEAD` or any commit-ish) of `repo`
    /// under the system temp directory and record this process as its owner.
    pub fn create(repo: &Path, rev: &str) -> Result<ThrowawayWorktree, String> {
        Self::create_with(repo, rev, crate::setup::branches::record_throwaway_owner)
    }

    /// (0.2.5 D7) [`ThrowawayWorktree::create`] at a GIVEN path: the worktree path of the run whose
    /// thread this run resumes (a worktree-isolated engine binds its session to the directory it
    /// was made in - kimi refuses a session "created under a different directory"). `at` must have
    /// C3's shape ([`reusable_worktree_path`]) and must be FREE: its directory is made exclusively,
    /// so anything already there - a live run of that thread, or a crashed run's leftover - is
    /// [`CreateError::Occupied`] and is never removed or reused from here; then `git worktree add`
    /// checks the revision out into that (empty) directory. `None` is a fresh random path, exactly
    /// as [`ThrowawayWorktree::create`]. Everything after the checkout (the owner record, the
    /// outside-link sweep, the removal when the guard drops) is the same.
    ///
    /// (0.2.5 A4, F57-7) The claimed directory keeps its identity through the checkout - for a
    /// given path and a fresh one alike. The system temp directory is opened ONCE; the path is
    /// built from it and the recorded name ([`reusable_worktree_path`]: the recorded parent must be
    /// that directory by file identity); the directory is made exclusively through the temp
    /// directory's handle and opened through it without following it, and that handle is kept
    /// ([`claim_dir`]: a link or a non-empty directory found there is [`CreateError::Replaced`]).
    /// After `git worktree add` the path is opened again by its name, without following it, and must
    /// be that same directory (identity of the two handles) before anything is staged or a reviewer
    /// starts; the root the run then uses IS that verified directory. A mismatch is
    /// [`CreateError::Replaced`]: nothing at the path is removed and the registration git made is
    /// pruned. A failed `git worktree add` removes only the claimed directory - through the temp
    /// directory's handle, while it is still that directory, and only when empty
    /// ([`remove_claimed`]). The limit: between the exclusive create and the open there is no handle
    /// yet, so an EMPTY real directory swapped in at that instant cannot be told from the one made
    /// (it holds nothing to lose; one with content, or a link, is refused).
    pub fn create_at(
        repo: &Path,
        rev: &str,
        at: Option<&Path>,
    ) -> Result<ThrowawayWorktree, CreateError> {
        Self::create_inner(
            repo,
            rev,
            at,
            crate::setup::branches::record_throwaway_owner,
        )
    }

    /// [`ThrowawayWorktree::create`] with the owner recorder given (a test injects a failing one).
    fn create_with(
        repo: &Path,
        rev: &str,
        record_owner: fn(&Path) -> Result<(), String>,
    ) -> Result<ThrowawayWorktree, String> {
        Self::create_inner(repo, rev, None, record_owner).map_err(|e| e.words())
    }

    /// The one creation: at a fresh random path (`at` = `None`) or at a given one
    /// ([`ThrowawayWorktree::create_at`]).
    fn create_inner(
        repo: &Path,
        rev: &str,
        at: Option<&Path>,
        record_owner: fn(&Path) -> Result<(), String>,
    ) -> Result<ThrowawayWorktree, CreateError> {
        let sha = resolve_commit(repo, rev).map_err(CreateError::Failed)?;
        // (R5, F18-2) the live repository's index is only read: no optional lock, no refresh
        let main_dirty = !git_capture(repo, &["--no-optional-locks", "status", "--porcelain"])
            .map_err(CreateError::Failed)?
            .is_empty();
        // (0.2.5 A4, F57-7) the temp directory, opened ONCE: the path is built from it and the
        // directory claimed through it
        let temp = open_temp_dir().map_err(CreateError::Failed)?;
        let (path, given) = match at {
            None => {
                let run_id = uuid::Uuid::new_v4().simple().to_string();
                (
                    temp.path()
                        .join(format!("{THROWAWAY_WORKTREE_PREFIX}{run_id}")),
                    false,
                )
            }
            Some(given) => {
                let p = reusable_under(&given.to_string_lossy(), &temp).map_err(|why| {
                    CreateError::Failed(format!(
                        "the worktree path {} cannot be reused: {why}",
                        given.display()
                    ))
                })?;
                (p, true)
            }
        };
        let name = path
            .file_name()
            .map(OsStr::to_os_string)
            .ok_or_else(|| CreateError::Failed(format!("{} has no name", path.display())))?;
        // (D7) the path is CLAIMED by making its directory exclusively: whatever already exists
        // there (a directory, a file, a link - dangling or not) is not ours and is left exactly as
        // it is; (A4) the claimed directory is kept by its handle from here on
        let claim = claim_dir(&temp, &name, &path, given)?;
        let path_s = path.to_string_lossy().to_string();
        interleave("worktree-add");
        if let Err(e) = git_capture(repo, &["worktree", "add", "--detach", &path_s, &sha]) {
            // (D7) the empty directory this call made - and only it - is removed again
            let left = remove_claimed(&temp, &name, &path, claim);
            return Err(CreateError::Failed(format!("{e}{left}")));
        }
        interleave("worktree-checkout");
        // (0.2.5 D4) The worktree's directory is opened once, WITHOUT following it, and verified on
        // that handle: everything C3 does in it from here on is relative to this handle.
        // (0.2.5 A4, F57-7) It must be the directory that was claimed: the identity of a fresh
        // no-follow open by the name against the kept handle, before anything is staged or any
        // reviewer starts. Anything else at the path is not this run's: nothing there is removed,
        // the registration git made is pruned, the run is refused.
        let root = match Root::open(&path) {
            Ok(r) if claim.is(r.dir()) => r,
            found => {
                let why = match found {
                    Ok(_) => "another directory sits there".to_string(),
                    Err(e) => format!("the path {}", e.words()),
                };
                drop(claim);
                let _ = git_capture(repo, &["worktree", "prune"]);
                return Err(CreateError::Replaced(path, why));
            }
        };
        drop(claim);
        drop(temp);
        // Name this process as its owner (in the worktree's git admin directory, not its files), so
        // `c3 branches` / `c3 doctor` can tell a crashed run's leftover from a live run's worktree.
        // (0.2.5 R2, F13-8) Not best effort: a worktree whose record cannot be written could not be
        // told from a leftover, so it is not used - it is removed (link-safely) and the run fails as
        // one whose worktree could not be made.
        if let Err(why) = record_owner(&path) {
            let removal =
                removal_words(remove_worktree_dir(repo, &path, Some(root), &REAL_REMOVAL));
            return Err(CreateError::Failed(format!(
                "its owner record could not be written ({why}); {removal}"
            )));
        }
        // (R5, F18-6) right after the checkout: a link the revision carries whose target lies
        // outside the worktree is removed (never followed, never descended), so a reviewer cannot
        // read or write the live checkout - or anything else - through it. A link that cannot be
        // removed makes the worktree unusable: it is removed again and the run fails as one whose
        // worktree could not be made.
        let outside_links = match unlink_outside_links(&root, contain::remove_link) {
            Ok(v) => v,
            Err(why) => {
                let removal =
                    removal_words(remove_worktree_dir(repo, &path, Some(root), &REAL_REMOVAL));
                return Err(CreateError::Failed(format!(
                    "a link of the checked-out revision that leads outside the worktree could not be removed ({why}); {removal}"
                )));
            }
        };
        Ok(ThrowawayWorktree {
            repo: repo.to_path_buf(),
            path,
            sha,
            main_dirty,
            outside_links,
            removal: None,
            root: Some(root),
        })
    }

    /// (R5, F18-6) The worktree-relative paths of the links removed right after the checkout
    /// because their target lay outside the worktree (ordinal-sorted).
    pub fn outside_links(&self) -> &[String] {
        &self.outside_links
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
        let mut l = self.isolation_line_base(rev_given);
        // (R5, F18-6) what the checkout carried that the reviewer could not follow
        if !self.outside_links.is_empty() {
            l.push_str(&format!(
                " {} link(s) of the checked-out revision that led outside the worktree were removed right after the checkout.",
                self.outside_links.len()
            ));
        }
        l
    }

    fn isolation_line_base(&self, rev_given: Option<&str>) -> String {
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
    /// path): its bytes are read first - its directory opened as an anchor, the file itself never
    /// followed ([`contain::read_path`]) - then written with [`ThrowawayWorktree::stage_bytes`].
    pub fn stage_file(&self, src: &Path, rel: &str) -> Result<(), String> {
        let bytes = contain::read_path(src)
            .map_err(|e| format!("{} cannot be read to be copied in ({e})", src.display()))?;
        self.stage_bytes(&bytes, rel)
    }

    /// The worktree's directory handle (opened at creation).
    fn root(&self) -> Result<&Root, String> {
        self.root
            .as_ref()
            .ok_or_else(|| format!("the worktree {} is not open", self.path.display()))
    }

    /// (F11-3) Write `bytes` into the worktree at the repo-relative path `rel` (a [`stageable_rel`]
    /// path) WITHOUT ever writing outside the worktree: every directory on the way must be a real
    /// directory inside the worktree - no component may be a symlink, a junction or any other
    /// reparse point (the checked-out revision may carry one) - and the destination must be absent
    /// or a regular file (never a link, a directory or anything else). The bytes go into a NEW file
    /// beside the destination (created exclusively under a random name, so nothing can be planted
    /// there), which then replaces the destination by a rename - the destination is never opened
    /// for writing, so a link or a hard link there cannot route the write elsewhere. The destination
    /// check is repeated right before the rename. Any doubt is an `Err`: the caller refuses the run
    /// before the reviewer starts.
    ///
    /// (0.2.5 D4, F32-1 / F24-1) Every step is relative to a directory HANDLE, never to the path
    /// name: each directory on the way is opened (or created) relative to its parent's handle
    /// without following it ([`Root::open_parent`]), the temporary file is created, the destination
    /// checked and the rename made relative to the destination directory's handle, and on a failure
    /// the temporary file is removed through it - a failed removal is reported in the `Err`
    /// ([`contain::publish`]). A directory swapped for a link after it was opened cannot redirect
    /// any of it: the write completes in the directory that was opened, inside the worktree.
    pub fn stage_bytes(&self, bytes: &[u8], rel: &str) -> Result<(), String> {
        if !stageable_rel(rel) {
            return Err(format!("`{rel}` is not a path inside the repository"));
        }
        let root = self.root()?;
        let (dir, leaf) = root
            .open_parent(Path::new(rel), true)
            .map_err(|e| self.step_words(rel, e))?;
        let check = |_: contain::Stage| dest_replaceable(&dir, &leaf, rel);
        let tmp = format!(".c3-stage-{}.tmp", uuid::Uuid::new_v4().simple());
        contain::publish(&contain::Publish {
            dir: &dir,
            leaf: &leaf,
            tmp: OsStr::new(&tmp),
            bytes,
            permissions: None,
            check: &check,
            race: "stage",
        })
        .map_err(|f| {
            let msg = match &f.error {
                contain::PublishError::Check(why) => why.clone(),
                contain::PublishError::Create(e) | contain::PublishError::Write(e) => {
                    format!("`{rel}` could not be written ({e})")
                }
                contain::PublishError::Rename(e) => {
                    format!("`{rel}` could not be put in place ({e})")
                }
            };
            f.with_cleanup(msg)
        })
    }

    /// A step of the way to a staging destination that did not give a real directory, in the words
    /// staging has always used.
    fn step_words(&self, rel: &str, e: StepError) -> String {
        let at = self.path.join(&e.at);
        match e.why {
            Step::Link { .. } => format!(
                "{} is a link in the worktree; nothing is copied through it",
                at.display()
            ),
            Step::NotDir | Step::NotFile => format!("{} is not a directory", at.display()),
            Step::Create(err) => format!("{} could not be created ({err})", at.display()),
            Step::BadPath => format!("`{rel}` is not a path inside the repository"),
            Step::Missing => format!("{} is not there", at.display()),
            Step::Io(err) => format!("{} cannot be opened ({err})", at.display()),
        }
    }

    /// Copy the directory `src` (recursively; a file `skip` names - its name is passed - is left
    /// out) into the worktree at the repo-relative path `rel`. (R5, F18-5) Nothing is ever read
    /// through a link: an entry that is a link (a symlink, a junction, any reparse point - read
    /// without following it) is neither followed nor copied, only counted. Returns how many files
    /// were copied and how many links were skipped.
    ///
    /// (0.2.5 D4, F29-2 / F18-5) `src` is an open directory HANDLE (the caller reached it from the
    /// repository's handle without following any component); each subdirectory is opened relative
    /// to its parent's handle without following it and each file read through a handle opened
    /// without following it and checked to be a regular file ([`contain::read_file_in`]) - an entry
    /// replaced by a link after it was listed is counted as a link, never read. `src_shown` names
    /// the source in messages only.
    pub fn stage_dir(
        &self,
        src: &cap_std::fs::Dir,
        src_shown: &Path,
        rel: &str,
        skip: &dyn Fn(&Path) -> bool,
    ) -> Result<StagedDir, String> {
        let mut staged = StagedDir::default();
        let first = src
            .try_clone()
            .map_err(|e| format!("{} cannot be opened ({e})", src_shown.display()))?;
        let mut stack: Vec<(cap_std::fs::Dir, PathBuf, String)> =
            vec![(first, src_shown.to_path_buf(), rel.to_string())];
        while let Some((dir, shown, dir_rel)) = stack.pop() {
            let names = contain::list(&dir)
                .map_err(|e| format!("{} cannot be listed ({e})", shown.display()))?;
            for name in names {
                let p = shown.join(&name);
                let child_rel = format!("{dir_rel}/{}", name.to_string_lossy());
                match contain::entry(&dir, &name) {
                    Ok(Some(Kind::Link { .. })) => staged.links_skipped += 1,
                    Ok(Some(Kind::Dir)) => match contain::open_child(&dir, &name, false) {
                        Ok(sub) => stack.push((sub, p, child_rel)),
                        Err(Step::Link { .. }) => staged.links_skipped += 1,
                        Err(Step::Missing) => {}
                        Err(why) => return Err(format!("{} {why}", p.display())),
                    },
                    Ok(Some(Kind::File)) if !skip(Path::new(&name)) => {
                        contain::race_point("stage-read");
                        match contain::read_file_in(&dir, &name) {
                            Ok(bytes) => {
                                self.stage_bytes(&bytes, &child_rel)?;
                                staged.files += 1;
                            }
                            Err(Step::Link { .. }) => staged.links_skipped += 1,
                            Err(Step::Missing) => {}
                            Err(why) => {
                                return Err(format!(
                                    "{} cannot be read to be copied in ({why})",
                                    p.display()
                                ))
                            }
                        }
                    }
                    // gone meanwhile, unreadable, skipped or not a file: not copied (as before)
                    _ => {}
                }
            }
        }
        Ok(staged)
    }

    /// Remove the worktree once ([`remove_worktree_dir`] with the real git and unlink); a refusal
    /// is logged in one line and returned. A second call returns the first call's outcome.
    pub fn remove(&mut self) -> Result<(), String> {
        if let Some(r) = &self.removal {
            return r.clone();
        }
        let root = self.root.take();
        let r = remove_worktree_dir(&self.repo, &self.path, root, &REAL_REMOVAL);
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

/// What [`ThrowawayWorktree::stage_dir`] copied: the files, and the links it skipped (never read
/// through).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StagedDir {
    pub files: usize,
    pub links_skipped: usize,
}

impl Drop for ThrowawayWorktree {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}

/// The git runner and the link remover of a throwaway worktree removal: the real ones in
/// production ([`REAL_REMOVAL`]); a test passes a git recorder or an injected unlink failure.
struct RemovalOps {
    git: fn(&Path, &[&str]) -> Result<String, String>,
    unlink: contain::UnlinkFn,
}

const REAL_REMOVAL: RemovalOps = RemovalOps {
    git: git_capture,
    unlink: contain::remove_link,
};

/// Remove a throwaway worktree of `repo` WITHOUT ever reaching outside it (F02-21, F03-2, F04-5).
/// First every link inside is unlinked ([`unlink_links_inside_with`]: every reparse point counts
/// as a link and is never descended into). If that pass hit ANY error - a link that could not be
/// removed, an unreadable entry - or the root itself is (now) a link, NOTHING destructive runs: no
/// file is removed, git is not called; `Err` says why and the directory stays for a later `c3
/// doctor --fix`. A path that does not exist at all is only pruned.
///
/// (0.2.5 D4, F16-5) The removal is bound to ONE directory handle: `root` is the handle the
/// worktree was opened by at creation (a leftover is opened here, without following it - a root
/// that is a link is refused). The pass, then the removal of everything inside
/// ([`contain::remove_contents`]: each file and link removed relative to its directory's handle,
/// each subdirectory opened without following it) and of the emptied directory itself
/// ([`contain::remove_root`]: the entry must still be the same directory) all go through it; then
/// `git worktree prune` drops the registration. No `git worktree remove --force` and no
/// `remove_dir_all` on a path name any more.
///
/// (0.2.5 R7b, F43-6) An entry that cannot be removed (a locked file, a denied delete) does not
/// stop the removal: everything else is removed, `Err` names up to ten entries left by their
/// worktree-relative paths, the worktree's `.git` file is kept (removed last, only when nothing
/// else is left - so `c3 doctor` still lists the half-removed worktree and `--fix` finishes it once
/// the obstacle is gone), and the registration is pruned only when the directory is gone.
fn remove_worktree_dir(
    repo: &Path,
    path: &Path,
    root: Option<Root>,
    ops: &RemovalOps,
) -> Result<(), String> {
    let root = match root {
        Some(r) => r,
        None => {
            if let Err(e) = std::fs::symlink_metadata(path) {
                if e.kind() == std::io::ErrorKind::NotFound {
                    let _ = (ops.git)(repo, &["worktree", "prune"]);
                    return Ok(());
                }
            }
            Root::open(path).map_err(|e| format!("the worktree root {}", e.words()))?
        }
    };
    let pass = unlink_links_inside_with(&root, ops.unlink);
    if !pass.errors.is_empty() {
        return Err(pass.summary());
    }
    contain::remove_contents(&root, ops.unlink, Some(OsStr::new(".git")))?;
    contain::remove_root(root)?;
    let _ = (ops.git)(repo, &["worktree", "prune"]);
    Ok(())
}

/// The result of one unlink pass: how many links were removed and every error met.
#[derive(Debug, Default)]
struct UnlinkPass {
    // (read by the tests; a removal acts on `errors` only)
    #[cfg_attr(not(test), allow(dead_code))]
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
/// cannot be read is refused. (0.2.5 D4) Read through the destination directory's handle.
fn dest_replaceable(dir: &cap_std::fs::Dir, leaf: &OsStr, rel: &str) -> Result<(), String> {
    match contain::entry(dir, leaf) {
        Ok(Some(Kind::Link { .. })) => Err(format!(
            "`{rel}` is a link in the worktree; nothing is copied through it"
        )),
        Ok(Some(Kind::File)) | Ok(None) => Ok(()),
        Ok(Some(_)) => Err(format!(
            "`{rel}` is not a regular file in the worktree; nothing is copied over it"
        )),
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

/// Remove every link INSIDE `root` without following it, through the handles
/// ([`contain::walk_links`]): each directory is listed through its own handle and each
/// subdirectory opened relative to its parent's handle without following it, so the walk never
/// descends into a link - one that replaces a directory after it was listed is met as a link and
/// unlinked - and a link ([`contain::Kind::Link`]: every reparse point on Windows) is removed with
/// `unlink` relative to its directory's handle. Every error (an unreadable directory or entry, a
/// link that could not be removed or is still there) is collected and the walk goes on. `root`
/// itself is not touched.
fn unlink_links_inside_with(root: &Root, unlink: contain::UnlinkFn) -> UnlinkPass {
    let mut removed = 0usize;
    let errors = contain::walk_links(root, false, &mut |l| {
        let at = root.path().join(l.dir_rel).join(l.name);
        match unlink(l.dir, l.name, l.dir_attr) {
            Ok(()) if contain::entry(l.dir, l.name).ok().flatten().is_none() => {
                removed += 1;
                Ok(())
            }
            Ok(()) => Err(format!("the link {} is still there", at.display())),
            Err(e) => Err(format!(
                "the link {} could not be removed ({e})",
                at.display()
            )),
        }
    });
    UnlinkPass { removed, errors }
}

/// (R5, F18-6) Remove every link INSIDE the fresh worktree `root` whose target does not resolve to
/// a path inside the worktree - a target outside it, or one that cannot be resolved (a dangling
/// link: a write through it would create its target wherever it points). A link ([`Kind::Link`]:
/// every reparse point on Windows) is never descended into - one whose canonical target lies
/// inside the worktree is left as the revision has it. Returns the removed links'
/// worktree-relative paths (ordinal-sorted); `Err` on the first entry that cannot be read or link
/// that cannot be removed (the caller does not use such a worktree).
///
/// (0.2.5 D4, F27-2) The walk goes through the handles ([`contain::walk_links`]): a directory
/// replaced by a junction or symlink after it was listed is met as a link, never descended, and
/// the unlink is relative to its directory's handle. Where a link leads is asked of the system
/// (it resolves the link) only to CLASSIFY it - nothing is read or written through it.
fn unlink_outside_links(root: &Root, unlink: contain::UnlinkFn) -> Result<Vec<String>, String> {
    let canon_root = std::fs::canonicalize(root.path())
        .map_err(|e| format!("the worktree root cannot be resolved ({e})"))?;
    let mut removed: Vec<String> = Vec::new();
    let errors = contain::walk_links(root, true, &mut |l| {
        let path = root.path().join(l.dir_rel).join(l.name);
        let inside = std::fs::canonicalize(&path)
            .map(|t| t.starts_with(&canon_root))
            .unwrap_or(false);
        if inside {
            return Ok(());
        }
        unlink(l.dir, l.name, l.dir_attr)
            .map_err(|e| format!("the link {} could not be removed ({e})", path.display()))?;
        if contain::entry(l.dir, l.name).ok().flatten().is_some() {
            return Err(format!("the link {} is still there", path.display()));
        }
        removed.push(l.dir_rel.join(l.name).to_string_lossy().replace('\\', "/"));
        Ok(())
    });
    if let Some(first) = errors.into_iter().next() {
        return Err(first);
    }
    removed.sort();
    Ok(removed)
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
        outside_links: Vec::new(),
        removal: None,
        root: None,
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
    let out = git_cmd(repo)
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
    let out = git_cmd(repo)
        .args(["cat-file", "blob", &format!("{rev}:{rel}")])
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

/// (0.2.5 D4) The repository-relative place of the live file `p`: its place by name under `repo`
/// (`.` and `..` resolved by name), else its canonical place; `None` when it is not below the
/// repository (or is the repository itself).
fn source_rel(repo: &Path, p: &Path) -> Option<PathBuf> {
    if let Some(r) = c3_core::paths::repo_relative(repo, p) {
        let mut parts: Vec<&str> = Vec::new();
        for c in r.split(['/', '\\']) {
            match c {
                "" | "." => {}
                ".." => {
                    parts.pop()?;
                }
                other => parts.push(other),
            }
        }
        return (!parts.is_empty()).then(|| parts.iter().collect());
    }
    let (c, r) = (
        std::fs::canonicalize(p).ok()?,
        std::fs::canonicalize(repo).ok()?,
    );
    let rel = c.strip_prefix(&r).ok()?;
    (!rel.as_os_str().is_empty()).then(|| rel.to_path_buf())
}

/// The refusal of a staging source read through a link: `leaf` - the file itself is the link -, or
/// a link on its way (named by whether the file then resolves outside the repository - that check
/// only words the message; nothing is read through the link either way).
fn linked_source_words(repo: &Path, p: &Path, leaf: bool) -> String {
    if leaf {
        return format!(
            "`{}` is a link (a symlink or a junction), and nothing is ever read through a link into the worktree",
            p.display()
        );
    }
    let inside = std::fs::canonicalize(p)
        .ok()
        .zip(std::fs::canonicalize(repo).ok())
        .is_some_and(|(c, r)| c.starts_with(r));
    if inside {
        format!(
            "`{}` is reached through a link (a symlink or a junction inside the repository), and nothing is ever read through a link into the worktree",
            p.display()
        )
    } else {
        format!(
            "`{}` resolves outside the repository (a link on its way), and nothing outside the repository is ever copied into the worktree",
            p.display()
        )
    }
}

/// (R5, F18-5) Why the live file `p` cannot be read for staging without reading through a link: it
/// is itself a link (a symlink, a junction, any reparse point - read without following it), or a
/// directory on its way is one (one leading out of the repository `repo`, or - 0.2.5 D4 - one that
/// stays inside it: C3 never reads through a link). `None` for a file that is not there at all (the
/// caller's fallbacks decide) or a regular file inside the repository.
///
/// (0.2.5 D4) The same walk the staging read does ([`contain::Root::kind`]): from the repository's
/// handle, one component at a time, none followed.
pub fn link_source_problem(repo: &Path, p: &Path) -> Option<String> {
    let Some(rel) = source_rel(repo, p) else {
        let meta = std::fs::symlink_metadata(p).ok()?;
        return Some(linked_source_words(repo, p, is_link(&meta)));
    };
    let root = Root::open_anchor(repo).ok()?;
    match root.kind(&rel) {
        Ok(Some(Kind::Link { .. })) => Some(linked_source_words(repo, p, true)),
        Err(StepError {
            why: Step::Link { .. },
            at,
        }) => Some(linked_source_words(repo, p, at == rel)),
        _ => None,
    }
}

/// What reading the live brief gave.
enum LiveSource {
    Bytes(Vec<u8>),
    /// It would be read through a link: refused (the words say why).
    Linked(String),
    /// It cannot be read (the committed copies may stand in).
    Unreadable(String),
}

/// (0.2.5 D4, F29-2 / F18-5) Read the live file `p` of the repository `repo` through handles: the
/// repository opened as an anchor, each directory on the way opened relative to its parent's handle
/// without following it, the file opened without following it and checked to be a regular file
/// on its handle ([`contain::Root::read`]) - the check IS the read, so nothing swapped in between
/// can be read through.
fn read_live_source(repo: &Path, p: &Path) -> LiveSource {
    let Some(rel) = source_rel(repo, p) else {
        return match std::fs::symlink_metadata(p) {
            Err(e) => LiveSource::Unreadable(e.to_string()),
            Ok(m) => LiveSource::Linked(linked_source_words(repo, p, is_link(&m))),
        };
    };
    let root = match Root::open_anchor(repo) {
        Ok(r) => r,
        Err(e) => {
            return LiveSource::Unreadable(format!(
                "the repository {} {}",
                repo.display(),
                e.words()
            ))
        }
    };
    match root.read(&rel) {
        Ok(b) => LiveSource::Bytes(b),
        Err(StepError {
            why: Step::Link { .. },
            at,
        }) => LiveSource::Linked(linked_source_words(repo, p, at == rel)),
        Err(StepError {
            why: Step::Io(e), ..
        }) => LiveSource::Unreadable(e.to_string()),
        Err(StepError {
            why: Step::Missing, ..
        }) => LiveSource::Unreadable("the file is not there".to_string()),
        Err(e) => LiveSource::Unreadable(format!("{} {}", e.at.display(), e.why)),
    }
}

/// (0.2.5 R8, F51-2) The bytes of the brief as the staging read them, and where it took them from:
/// `from_revision` is `None` for the live file, else the full sha of the commit whose regular file
/// was materialised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BriefSource {
    pub bytes: Vec<u8>,
    pub from_revision: Option<String>,
}

/// (F11-4) The bytes of the brief as the reviewer's worktree gets them: the live file when it can
/// be read, else the regular file committed at a revision ([`committed_brief`]: (0.2.5 R8, F51-2)
/// `rev` - the worktree's own revision, under `--rev` the reviewed one - first, then the live
/// checkout's `HEAD`), with where they came from. `Err` names why none is there. (R5, F18-5) A live
/// brief that is a link, or reached through one, is an `Err` at once - never read, and not replaced
/// by a committed copy either. (0.2.5 D4) The live file is read through handles
/// ([`read_live_source`]).
pub fn brief_bytes(
    repo: &Path,
    brief_ref: &str,
    brief_path: Option<&Path>,
    rev: &str,
) -> Result<BriefSource, String> {
    let live_err = match brief_path {
        Some(p) => match read_live_source(repo, p) {
            LiveSource::Bytes(bytes) => {
                return Ok(BriefSource {
                    bytes,
                    from_revision: None,
                })
            }
            LiveSource::Linked(why) => return Err(why),
            LiveSource::Unreadable(e) => e,
        },
        None => "no brief file was resolved".to_string(),
    };
    if let Some((sha, bytes)) = committed_brief(repo, brief_ref, rev) {
        return Ok(BriefSource {
            bytes,
            from_revision: Some(sha),
        });
    }
    Err(format!(
        "it cannot be read ({live_err}) and no regular file of that path is committed at HEAD or at the reviewed revision"
    ))
}

/// (0.2.5 D1, F10-3 / F11-4) Where the staging takes a brief from when the live tree does not hold
/// it: the commit whose regular file at `brief_ref` it materialises - (0.2.5 R8, F51-2) `rev` (the
/// worktree's own revision: under `--rev` the reviewed one, whose own copy of the brief wins) first,
/// else the live checkout's `HEAD` - as its FULL sha (resolved once, the blob read at that sha), with
/// those bytes. `None` when the reference is empty, absolute or leaves the repository, or neither
/// revision commits a regular file there.
pub fn committed_brief(repo: &Path, brief_ref: &str, rev: &str) -> Option<(String, Vec<u8>)> {
    if brief_ref.is_empty() || Path::new(brief_ref).is_absolute() || !stageable_rel(brief_ref) {
        return None;
    }
    let mut tried: Vec<String> = Vec::new();
    for r in [rev, "HEAD"] {
        let Ok(sha) = resolve_commit(repo, r) else {
            continue;
        };
        if tried.contains(&sha) {
            continue;
        }
        if let Some(b) = committed_blob(repo, &sha, brief_ref) {
            return Some((sha, b));
        }
        tried.push(sha);
    }
    None
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
    } else if let Some(p) = brief_path.and_then(|bp| link_source_problem(repo, bp)) {
        // (R5, F18-5) a linked brief is never read: the seat is refused before anything starts
        return Some(staging_refusal(
            engine,
            &format!("the brief `{brief_ref}`"),
            rev_label,
            &p,
        ));
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
    uncommitted_paths_outside(repo, None)
}

/// (0.2.5 D1) Whether the repo-relative path `p` (`/`) is `dir` or lies under it (a directory git
/// shows with a trailing `/` included), compared case-insensitively on Windows and macOS.
fn path_under(p: &str, dir: &str) -> bool {
    let dir = dir.trim_end_matches('/');
    if dir.is_empty() {
        return false;
    }
    let ci = cfg!(any(windows, target_os = "macos"));
    let (p, d) = if ci {
        (p.to_lowercase(), dir.to_lowercase())
    } else {
        (p.to_string(), dir.to_string())
    };
    p == d || p.starts_with(&format!("{d}/"))
}

/// (0.2.5 D1) [`uncommitted_paths`] without every path under `skip_dir` (repo-relative, `/`):
/// left out of the tracked list AND of the count of new paths.
pub fn uncommitted_paths_outside(repo: &Path, skip_dir: Option<&str>) -> Uncommitted {
    let skipped = |p: &str| skip_dir.is_some_and(|d| path_under(p, d));
    let out = match git_cmd(repo)
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
                if !skipped(orig) {
                    u.tracked.push(orig.to_string());
                }
            }
            continue;
        }
        if skipped(&path) {
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
/// instruction line) and - (0.2.5 A4, F57-8) - no Unicode format character
/// ([`crate::unicode::is_format_char`]: a bidi override or an invisible character could reorder or
/// hide what the name shows); `None` when it must be counted instead.
fn prompt_safe_path(p: &str) -> Option<String> {
    if p.is_empty()
        || p.chars()
            .any(|c| c == '`' || c.is_control() || crate::unicode::is_format_char(c))
    {
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
/// (0.2.5 R8, F53-1) The paragraph is ALWAYS there for an isolated run - the reviewer is told its
/// working directory is a throwaway worktree of HEAD, not the live checkout, and that the brief was
/// copied in; only the list and the counts of uncommitted paths depend on the live tree.
pub fn isolation_prompt_line(
    head_short: &str,
    uncommitted: &Uncommitted,
    brief_ref: &str,
) -> String {
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
/// HEAD) - (0.2.5 R8, F53-1) also when the tree is clean. (0.2.5 D1) Every path under the collab
/// directory (`collab_rel`, repo-relative; the task's handoffs are staged into the worktree) is left
/// out of the list and out of its count.
pub fn isolation_prompt(repo: &Path, brief_ref: &str, collab_rel: Option<&str>) -> String {
    let paths = uncommitted_paths_outside(repo, collab_rel);
    let head = git_capture(repo, &["rev-parse", "--short=12", "HEAD"]).unwrap_or_default();
    isolation_prompt_line(&head, &paths, brief_ref)
}

/// (B1) The prompt paragraph of a reviewer under `--rev <rev>`: its working directory is a
/// throwaway worktree of that revision (the full sha), not the live checkout; the brief (when it is
/// a repo-relative path) and the task's handoffs were copied in at their paths. (R5, F18-7)
/// `staged_handoffs` is the task directory's repo-relative path ONLY when a `handoffs/` directory
/// was actually staged; `None` otherwise, and then the handoffs are not mentioned.
pub fn rev_prompt_line(
    sha: &str,
    rev: &str,
    brief_ref: &str,
    staged_handoffs: Option<&str>,
) -> String {
    let mut l = format!(
        "Isolation: your working directory is a throwaway git worktree of revision {sha} (`--rev {rev}`), not the live checkout: review the code as it is at that revision."
    );
    let brief = !brief_ref.is_empty() && stageable_rel(brief_ref);
    match (brief, staged_handoffs) {
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

/// (R5, F18-6) The prompt sentence naming the links removed right after the checkout because they
/// led outside the worktree: how many, at most [`UNCOMMITTED_PATHS_SHOWN`] of them named - each as
/// data ([`prompt_safe_path`]: a name that could break out of its code span is counted, never
/// shown) - and the rest counted. `""` when none was removed.
pub fn outside_links_sentence(links: &[String]) -> String {
    if links.is_empty() {
        return String::new();
    }
    let mut shown: Vec<String> = Vec::new();
    let mut withheld = 0usize;
    for p in links {
        match prompt_safe_path(p) {
            Some(s) if shown.len() < UNCOMMITTED_PATHS_SHOWN => shown.push(s),
            _ => withheld += 1,
        }
    }
    let mut l = format!(
        "{} link(s) of the checked-out revision led outside the worktree and were removed right after the checkout, so their targets are not part of what you review",
        links.len()
    );
    if !shown.is_empty() {
        l.push_str(&format!(": {}", shown.join(", ")));
    }
    if withheld > 0 {
        l.push_str(&format!(" (+{withheld} more)"));
    }
    l.push('.');
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
        let pass = {
            let root = Root::open(&wt).unwrap();
            unlink_links_inside_with(&root, contain::remove_link)
        };
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

    /// Remove ONE link by its path (test clean-up only; never its target).
    fn remove_link_path(p: &Path) {
        let _ = std::fs::remove_dir(p).or_else(|_| std::fs::remove_file(p));
    }

    /// An injected unlink failure (a link held open, a denied delete).
    fn failing_unlink(
        _dir: &cap_std::fs::Dir,
        _name: &OsStr,
        _dir_attr: bool,
    ) -> std::io::Result<()> {
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
        let r = remove_worktree_dir(&base, &wt, None, &ops);
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
        // the same tree with the real unlink is removed cleanly: (0.2.5 D4) through the handles,
        // then only `git worktree prune` for the registration - never a forced `worktree remove`
        let ok = RemovalOps {
            git: recording_git,
            unlink: contain::remove_link,
        };
        assert_eq!(remove_worktree_dir(&base, &wt, None, &ok), Ok(()));
        assert_eq!(git_calls(), vec!["worktree prune".to_string()]);
        assert!(
            std::fs::symlink_metadata(&wt).is_err(),
            "the removal through the handles removed it"
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
            unlink: contain::remove_link,
        };
        let why = remove_worktree_dir(&base, &wt, None, &ops).expect_err("a link root is refused");
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
        remove_link_path(&inner);
        remove_link_path(&wt);
        assert!(outside.join("sentinel.txt").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    // The production removal end to end on a real git worktree: a reviewer left a directory link to
    // an outside sentinel; the guard unlinks it, the worktree is removed through its handles and
    // `git worktree prune` drops its registration, and the sentinel survives.
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

    // (R5, F18-2 / F20-2, the reviewers' RC1 / RC4) setting up and checking a review worktree never
    // rewrites or locks the live repository's index: with the index's stat cache stale (a tracked
    // file touched, its content unchanged - a plain `git status` would refresh and rewrite the
    // index), creating a worktree, staging into it and checking it leave the index's bytes as they
    // were and no `index.lock` behind; a revision that reads as an option is refused, and is never
    // handed to git as one.
    #[test]
    fn a_review_worktree_never_rewrites_the_live_index() {
        let repo = brief_repo("index");
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(repo.join("committed.md"))
            .unwrap();
        f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600))
            .unwrap();
        drop(f);
        let index = repo.join(".git").join("index");
        let before = std::fs::read(&index).unwrap();
        let wt = ThrowawayWorktree::create(&repo, "HEAD").unwrap();
        wt.stage_bytes(b"brief\n", "docs/brief.md").unwrap();
        let _ = uncommitted_paths(&repo);
        let _ = isolation_prompt(&repo, "docs/brief.md", None);
        let _ = rev_destination_problem(&repo, wt.sha(), "docs/brief.md");
        let _ = committed_blob(&repo, "HEAD", "committed.md");
        assert_eq!(
            std::fs::read(&index).unwrap(),
            before,
            "the live index was rewritten"
        );
        assert!(!repo.join(".git").join("index.lock").exists());
        drop(wt);
        // the same refresh without the guard WOULD rewrite it (the scenario is real), shown only
        let plain = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["status", "--porcelain"])
            .output()
            .unwrap();
        assert!(plain.status.success());
        if std::fs::read(&index).unwrap() == before {
            eprintln!("note: this git did not refresh the stale index on a plain status");
        }
        for bad in ["--output=x", "-c", "HEAD\nx"] {
            assert!(resolve_commit(&repo, bad).is_err(), "{bad:?}");
            assert!(ThrowawayWorktree::create(&repo, bad).is_err(), "{bad:?}");
        }
        assert!(!repo.join("x").exists());
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
        let src_root = Root::open_anchor(&src).unwrap();
        let n = wt
            .stage_dir(src_root.dir(), &src, ".collab/t/handoffs", &skip)
            .unwrap();
        drop(src_root);
        assert_eq!(
            n,
            StagedDir {
                files: 2,
                links_skipped: 0
            }
        );
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
                remove_link_path(&live_path);
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
            at: None,
            resume_note: String::new(),
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

    // (F11-3, R5 F18-6) a link that appears in the worktree's checkout (here planted by a
    // post-checkout hook) on the way to the brief, leading OUTSIDE the worktree: it is removed right
    // after the checkout (never followed), counted and named, the brief is staged into a real
    // directory of the worktree, and nothing is written through the link - the outside file keeps
    // its bytes. A link that stays (here made after the checkout) still refuses the staging.
    #[test]
    fn a_link_in_the_checkout_leading_outside_is_removed_and_the_brief_staged_inside() {
        let repo = brief_repo("hook-link");
        let outside = repo.parent().unwrap().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("brief.md"), "outside brief\n").unwrap();
        std::fs::write(outside.join("sentinel.txt"), "keep me\n").unwrap();
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
            at: None,
            resume_note: String::new(),
        };
        let collab = repo.join(".collab");
        let tree = crate::consult::reviewed::setup_review_tree(
            &plan,
            &crate::consult::reviewed::StageInputs {
                repo_root: &repo,
                collab_root: &collab,
                task: "t",
                brief_ref: "docs/brief.md",
                brief_path: Some(&repo.join("docs").join("brief.md")),
                engine: "kimi",
            },
        )
        .unwrap_or_else(|e| panic!("the outside link was not removed at checkout: {e:?}"));
        assert_eq!(tree.outside_links(), ["docs".to_string()]);
        let docs = tree.path().join("docs");
        let m = std::fs::symlink_metadata(&docs).unwrap();
        assert!(m.is_dir() && !is_link(&m), "a real directory now");
        assert_eq!(
            std::fs::read_to_string(docs.join("brief.md")).unwrap(),
            "# live brief\n"
        );
        assert!(tree
            .isolation_line(&plan)
            .ends_with("1 link(s) of the checked-out revision that led outside the worktree were removed right after the checkout."));
        // (a link that appears AFTER the checkout on the brief's way is still refused:
        // `staging_never_writes_through_a_link_in_the_worktree`)
        let wt_path = tree.path().to_path_buf();
        drop(tree);
        assert!(std::fs::symlink_metadata(&wt_path).is_err());
        assert_eq!(
            std::fs::read_to_string(outside.join("brief.md")).unwrap(),
            "outside brief\n"
        );
        assert_eq!(
            std::fs::read_to_string(outside.join("sentinel.txt")).unwrap(),
            "keep me\n",
            "the removal never followed the link"
        );
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }

    // (R5, F18-6, the reviewer's RC5) a link the revision carries whose target is the LIVE checkout
    // is unlinked right after the checkout - the reviewer can neither read nor write the live tree
    // through it; a link whose target lies inside the worktree is left as the revision has it; the
    // prompt names the removed ones as data.
    #[test]
    fn links_leading_out_of_the_worktree_are_removed_right_after_the_checkout() {
        let repo = brief_repo("out-links");
        // a committed link to the live checkout's file, and one to a file inside the revision
        commit_link_entry(&repo, "out", &repo.join("committed.md"), "live\n");
        let wt = ThrowawayWorktree::create(&repo, "HEAD").unwrap();
        let out = wt.path().join("out");
        match std::fs::symlink_metadata(&out) {
            Ok(m) if is_link(&m) => panic!("the outside link survived the checkout"),
            Ok(_) => eprintln!(
                "the checkout made a plain file of the committed link (core.symlinks is off): only the planted-link half applies"
            ),
            Err(_) => assert_eq!(wt.outside_links(), ["out".to_string()]),
        }
        drop(wt);
        // a planted directory link to the live checkout (a junction needs no privilege) and a
        // link inside the worktree that stays
        let repo2 = brief_repo("out-links-2");
        plant_dir_link_on_checkout(&repo2, &repo2);
        let wt2 = ThrowawayWorktree::create(&repo2, "HEAD").unwrap();
        assert_eq!(wt2.outside_links(), ["docs".to_string()]);
        assert!(std::fs::symlink_metadata(wt2.path().join("docs")).is_err());
        assert_eq!(
            std::fs::read_to_string(repo2.join("committed.md")).unwrap(),
            "# c\n"
        );
        std::fs::create_dir_all(wt2.path().join("inner")).unwrap();
        make_inner_dir_link(&wt2.path().join("inner-link"), &wt2.path().join("inner"));
        assert_eq!(
            unlink_outside_links(wt2.root.as_ref().unwrap(), contain::remove_link).unwrap(),
            Vec::<String>::new(),
            "a link inside the worktree stays"
        );
        assert!(std::fs::symlink_metadata(wt2.path().join("inner-link")).is_ok());
        // the prompt names them as data, a name that could break out is counted
        let s = outside_links_sentence(&["docs".into(), "a`b".into()]);
        assert!(
            s.starts_with("2 link(s) of the checked-out revision led outside the worktree")
                && s.ends_with(": `docs` (+1 more)."),
            "{s}"
        );
        assert_eq!(outside_links_sentence(&[]), "");
        drop(wt2);
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
        let _ = std::fs::remove_dir_all(repo2.parent().unwrap());
    }

    /// Every entry below `dir` (links not followed), as paths relative to it.
    fn tree_names(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                out.push(
                    p.strip_prefix(dir)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
                if std::fs::symlink_metadata(&p).is_ok_and(|m| m.is_dir() && !is_link(&m)) {
                    stack.push(p);
                }
            }
        }
        out.sort();
        out
    }

    /// No staging or repair temporary file anywhere below `dir`.
    fn no_temp_below(dir: &Path) -> bool {
        !tree_names(dir)
            .iter()
            .any(|n| n.contains(".c3-stage-") || n.ends_with(".tmp"))
    }

    // (0.2.5 D4, F32-1 / F24-1) THE STAGING RACE: the destination directory is swapped for a link
    // to an outside directory right after it was opened - before the temporary file is created
    // (`stage-create`), and before it is renamed into place (`stage-publish`). The write never
    // reaches the outside directory and no temporary file is left anywhere: on Windows the swap
    // itself is refused (C3 holds the directory's handle, so it cannot be renamed); elsewhere the
    // swap happens and the file lands in the ORIGINAL directory - the one the handle names, now under
    // its aside name - strictly inside the worktree. Either way the staging completes.
    #[test]
    fn staging_into_a_directory_swapped_for_a_link_never_leaves_the_worktree() {
        for point in ["stage-create", "stage-publish"] {
            let repo = brief_repo("race-stage");
            let outside = repo.parent().unwrap().join("outside");
            std::fs::create_dir_all(&outside).unwrap();
            std::fs::write(outside.join("sentinel.txt"), "keep me\n").unwrap();
            let wt = ThrowawayWorktree::create(&repo, "HEAD").unwrap();
            let docs = wt.path().join("docs");
            std::fs::create_dir_all(&docs).unwrap();
            let hook = c3_core::test_hooks::set_thread_hook(
                contain::RACE_HOOK,
                &format!("{point}|{}|{}", docs.display(), outside.display()),
            );
            let r = wt.stage_bytes(b"the operator's brief\n", "docs/brief.md");
            drop(hook);
            let aside = wt.path().join("docs.c3-race-orig");
            let swapped = aside.exists();
            if cfg!(windows) {
                assert!(
                    !swapped,
                    "{point}: a directory whose handle is held cannot be renamed on Windows"
                );
            }
            assert_eq!(r, Ok(()), "{point}: completes in the directory it opened");
            let landed = if swapped { &aside } else { &docs };
            assert_eq!(
                std::fs::read_to_string(landed.join("brief.md")).unwrap(),
                "the operator's brief\n",
                "{point}"
            );
            assert_eq!(tree_names(&outside), vec!["sentinel.txt".to_string()]);
            assert_eq!(
                std::fs::read_to_string(outside.join("sentinel.txt")).unwrap(),
                "keep me\n"
            );
            assert!(
                no_temp_below(wt.path()),
                "{point}: {:?}",
                tree_names(wt.path())
            );
            let wt_path = wt.path().to_path_buf();
            drop(wt);
            assert!(std::fs::symlink_metadata(&wt_path).is_err(), "{point}");
            assert_eq!(tree_names(&outside), vec!["sentinel.txt".to_string()]);
            let _ = std::fs::remove_dir_all(repo.parent().unwrap());
        }
    }

    // (0.2.5 D4, F27-2 / F16-5) THE WALK RACE: a subdirectory is swapped for a link to an outside
    // directory after the walk listed it and before it descends (`walk-descend`) - in the removal's
    // link pass, in the post-checkout sweep and in the whole removal. The walk never descends
    // through it: the link inside the outside directory (`trap`) and the sentinel are untouched, the
    // swapped-in link itself is unlinked (never followed), the original directory is kept (or, in
    // the whole removal, removed as part of the worktree). Where the system refuses the swap, the
    // walk simply descends into the real directory.
    #[test]
    fn a_directory_swapped_for_a_link_during_the_walk_is_never_descended() {
        for walker in ["pass", "sweep", "removal"] {
            let (base, wt, outside) = link_scratch(&format!("race-walk-{walker}"));
            let elsewhere = base.join("elsewhere");
            std::fs::create_dir_all(&elsewhere).unwrap();
            let trap = outside.join("trap");
            make_inner_dir_link(&trap, &elsewhere);
            let sub = wt.join("sub");
            let hook = c3_core::test_hooks::set_thread_hook(
                contain::RACE_HOOK,
                &format!("walk-descend|{}|{}", sub.display(), outside.display()),
            );
            let swapped_links: usize = match walker {
                "pass" => {
                    let root = Root::open(&wt).unwrap();
                    let pass = unlink_links_inside_with(&root, contain::remove_link);
                    assert!(pass.errors.is_empty(), "{:?}", pass.errors);
                    pass.removed
                }
                "sweep" => {
                    let root = Root::open(&wt).unwrap();
                    unlink_outside_links(&root, contain::remove_link)
                        .unwrap()
                        .len()
                }
                _ => {
                    let ops = RemovalOps {
                        git: recording_git,
                        unlink: contain::remove_link,
                    };
                    assert_eq!(remove_worktree_dir(&base, &wt, None, &ops), Ok(()));
                    assert!(std::fs::symlink_metadata(&wt).is_err(), "removed");
                    0
                }
            };
            drop(hook);
            assert!(
                std::fs::symlink_metadata(&trap).is_ok(),
                "{walker}: the link inside the outside directory was never touched"
            );
            assert_eq!(
                tree_names(&outside),
                vec!["sentinel.txt".to_string(), "trap".to_string()],
                "{walker}"
            );
            assert_eq!(
                std::fs::read_to_string(outside.join("sentinel.txt")).unwrap(),
                "keep me\n"
            );
            if walker != "removal" {
                let aside = wt.join("sub.c3-race-orig");
                if aside.exists() {
                    assert_eq!(
                        swapped_links, 1,
                        "{walker}: the swapped-in link was unlinked"
                    );
                    assert!(std::fs::symlink_metadata(&sub).is_err(), "{walker}");
                    assert!(
                        aside.join("own.txt").exists(),
                        "{walker}: the original is kept"
                    );
                } else {
                    assert_eq!(swapped_links, 0, "{walker}");
                    assert!(sub.join("own.txt").exists(), "{walker}");
                }
            }
            remove_link_path(&trap);
            let _ = std::fs::remove_dir_all(&base);
        }
    }

    /// A directory link at `link` to `target` inside the same tree: a junction on Windows, a symlink
    /// elsewhere (no sentinel check, unlike [`make_dir_link`]).
    fn make_inner_dir_link(link: &Path, target: &Path) {
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
            String::from_utf8_lossy(&b.bytes).replace("\r\n", "\n"),
            "# committed brief\n"
        );
        // (0.2.5 R8, F51-2) named by the full sha of the commit it came from
        let head_sha = git_capture(&repo, &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(b.from_revision.as_deref(), Some(head_sha.trim()));
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
            at: None,
            resume_note: String::new(),
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
        // (0.2.5 R8, F51-2) the record names the bytes the staging wrote and where it took them
        let staged = tree.staged_brief().unwrap();
        assert_eq!(
            staged.0,
            c3_core::sha256_hex(&std::fs::read(tree.path().join("docs").join("brief.md")).unwrap())
        );
        assert_eq!(staged.1.as_deref(), Some(head_sha.trim()));
        drop(tree);
        // under --rev the reviewed revision's own copy wins over HEAD's
        std::fs::write(repo.join("docs").join("brief.md"), "# newer brief\n").unwrap();
        git_capture(&repo, &["commit", "-q", "-am", "newer brief"]).unwrap();
        let older = head_sha.trim().to_string();
        let b = brief_bytes(&repo, "docs/brief.md", Some(&missing), &older).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&b.bytes).replace("\r\n", "\n"),
            "# committed brief\n"
        );
        assert_eq!(b.from_revision.as_deref(), Some(older.as_str()));
        // a live file is staged as it is, with no revision named
        let live = repo.join("docs").join("brief.md");
        let b = brief_bytes(&repo, "docs/brief.md", Some(&live), &older).unwrap();
        assert_eq!(b.from_revision, None);
        assert_eq!(
            String::from_utf8_lossy(&b.bytes).replace("\r\n", "\n"),
            "# newer brief\n"
        );
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
        // (0.2.5 R8, F53-1) a clean live tree: the reviewer is still told where it is, and that the
        // brief was copied in - only the path list is left out
        assert_eq!(
            isolation_prompt_line("abc", &Uncommitted::default(), "b.md"),
            "Isolation: your working directory is a throwaway git worktree of the committed HEAD abc, not the live checkout. The brief `b.md` was copied into your worktree at that path."
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
        // names with a backtick, a line break, a control character or (0.2.5 A4, F57-8) a Unicode
        // format character (here U+202E, the right-to-left override) are counted, not shown
        let odd = Uncommitted {
            tracked: vec![
                "ok.rs".into(),
                "a`b.rs".into(),
                "line\nbreak.rs".into(),
                "tab\tx.rs".into(),
                "invoice\u{202E}txt.exe".into(),
            ],
            new_paths: 0,
        };
        let l = isolation_prompt_line("abc", &odd, "");
        assert!(
            l.contains("5 tracked path(s) had uncommitted changes"),
            "{l}"
        );
        assert!(l.contains(": `ok.rs` (+4 more)."), "{l}");
        assert!(
            !l.contains("a`b") && !l.contains("break.rs") && !l.contains("tab"),
            "{l}"
        );
        assert!(
            !l.contains('\u{202E}') && !l.contains("invoice") && !l.contains("txt.exe"),
            "the name with the override is counted, none of its bytes shown: {l:?}"
        );
        // from a live repo (the reviewer's Verify): a modified tracked file is named, a
        // confidential untracked file is only counted.
        let repo = brief_repo("iso");
        let clean = isolation_prompt(&repo, "", None);
        assert!(
            clean.starts_with("Isolation: your working directory is a throwaway git worktree of the committed HEAD ")
                && clean.ends_with(", not the live checkout."),
            "{clean}"
        );
        std::fs::write(repo.join("committed.md"), "# changed\n").unwrap();
        std::fs::write(repo.join("secret-acquisition-plan.txt"), "n\n").unwrap();
        assert_eq!(
            uncommitted_paths(&repo),
            Uncommitted {
                tracked: vec!["committed.md".to_string()],
                new_paths: 1
            }
        );
        let live = isolation_prompt(&repo, "", None);
        assert!(live.contains(": `committed.md`."), "{live}");
        assert!(live.contains(" 1 path(s) that are not in HEAD"), "{live}");
        assert!(!live.contains("secret-acquisition-plan"), "{live}");
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }

    // (0.2.5 D1) The paragraph leaves every path under the collab directory out of the list and out
    // of its counts: a tracked handoff changed in the live tree and an untracked collab directory
    // are not named and not counted (the task's handoffs are staged into the worktree).
    #[test]
    fn the_isolation_prompt_leaves_the_collab_directory_out() {
        let repo = brief_repo("iso-collab");
        std::fs::create_dir_all(repo.join(".collab/t/handoffs")).unwrap();
        std::fs::write(repo.join(".collab/t/handoffs/01-brief.md"), "v1\n").unwrap();
        git_capture(&repo, &["add", "-f", ".collab"]).unwrap();
        git_capture(&repo, &["commit", "-q", "-m", "a tracked handoff"]).unwrap();
        std::fs::write(repo.join(".collab/t/handoffs/01-brief.md"), "v2\n").unwrap();
        std::fs::write(repo.join(".collab/t/handoffs/02-new.md"), "new\n").unwrap();
        std::fs::create_dir_all(repo.join("collab-elsewhere")).unwrap();
        std::fs::write(repo.join("collab-elsewhere/x.md"), "x\n").unwrap();
        std::fs::write(repo.join("committed.md"), "# changed\n").unwrap();
        let all = uncommitted_paths(&repo);
        assert!(
            all.tracked
                .contains(&".collab/t/handoffs/01-brief.md".to_string()),
            "{all:?}"
        );
        let u = uncommitted_paths_outside(&repo, Some(".collab"));
        assert_eq!(u.tracked, vec!["committed.md".to_string()], "{u:?}");
        // only `collab-elsewhere/` is new outside the collab directory
        assert_eq!(u.new_paths, 1, "{u:?}");
        let l = isolation_prompt(&repo, "", Some(".collab/"));
        assert!(
            l.contains("1 tracked path(s) had uncommitted changes") && l.contains("`committed.md`"),
            "{l}"
        );
        assert!(!l.contains(".collab"), "{l}");
        assert!(l.contains(" 1 path(s) that are not in HEAD"), "{l}");
        assert!(path_under(".collab", ".collab") && path_under(".collab/t/x", ".collab/"));
        assert!(!path_under(".collabx/t", ".collab") && !path_under("a/.collab/x", ".collab"));
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

    /// A path of C3's throwaway shape (directly under the system temp directory, the prefix and a
    /// fresh 32-hex id) that does not exist yet.
    fn c3_shaped_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "{THROWAWAY_WORKTREE_PREFIX}{}",
            uuid::Uuid::new_v4().simple()
        ))
    }

    // (0.2.5 D7) The shape rule of a reusable worktree path: exactly C3's own shape - directly under
    // the system temp directory, the prefix and 32 lower-case hex digits, an absolute path without
    // `.` / `..` - is reusable; a path outside temp, a wrong prefix, a wrong length, upper-case hex,
    // a relative path or one with `..` is refused, each saying why. (0.2.5 A4, F57-7) The path is
    // the temp directory joined with the recorded name.
    #[test]
    fn a_reusable_worktree_path_has_exactly_c3s_shape() {
        let ok = c3_shaped_path();
        assert_eq!(
            reusable_worktree_path(&ok.to_string_lossy()),
            Ok(ok.clone())
        );
        let id = uuid::Uuid::new_v4().simple().to_string();
        let temp = std::env::temp_dir();
        // outside temp: one directory deeper (an existing directory, so only the place is wrong)
        let base = brief_repo("shape");
        let outside = base.join(format!("{THROWAWAY_WORKTREE_PREFIX}{id}"));
        let why = reusable_worktree_path(&outside.to_string_lossy()).unwrap_err();
        assert!(
            why.contains("not directly under the system temp directory"),
            "{why}"
        );
        // a parent that does not exist is not the temp directory either
        let missing = temp
            .join(format!("no-such-{id}"))
            .join(format!("{THROWAWAY_WORKTREE_PREFIX}{id}"));
        assert!(reusable_worktree_path(&missing.to_string_lossy()).is_err());
        // the name: a wrong prefix, a wrong length (31 and 33), upper-case hex, a user's suffix
        for name in [
            format!("c3-kimi-wx-{id}"),
            format!("{THROWAWAY_WORKTREE_PREFIX}{}", &id[1..]),
            format!("{THROWAWAY_WORKTREE_PREFIX}{id}0"),
            format!("{THROWAWAY_WORKTREE_PREFIX}{}", id.to_uppercase()),
            format!("{THROWAWAY_WORKTREE_PREFIX}notes"),
        ] {
            let why = reusable_worktree_path(&temp.join(&name).to_string_lossy()).unwrap_err();
            assert!(why.contains("its name is not"), "{name}: {why}");
        }
        // a relative path, a `..` detour that lands in temp again, and nothing recorded
        let rel = reusable_worktree_path(&format!("{THROWAWAY_WORKTREE_PREFIX}{id}")).unwrap_err();
        assert!(rel.contains("not an absolute path"), "{rel}");
        let detour = temp
            .join("sub")
            .join("..")
            .join(format!("{THROWAWAY_WORKTREE_PREFIX}{id}"));
        let why = reusable_worktree_path(&detour.to_string_lossy()).unwrap_err();
        assert!(why.contains("`..`"), "{why}");
        assert!(reusable_worktree_path("").is_err());
        let _ = std::fs::remove_dir_all(base.parent().unwrap());
    }

    // (0.2.5 A4, F57-7) THE PARENT-IDENTITY RULE: "directly under the temp directory" is the file
    // identity of the two opened directories, never path text. A directory that merely LOOKS like
    // temp (another directory with temp's own name) is not reused; a recorded parent that reaches
    // the temp directory by another spelling (an alias link to it) is the temp directory, and the
    // path the worktree is made at is BUILT from the temp directory and the name - never the
    // recorded spelling. On Unix a literal backslash stays part of a name: a directory named `a\b`
    // is not `a/b` (no separator rewriting).
    #[test]
    fn a_reusable_path_is_built_from_the_temp_directory_by_identity() {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let name = format!("{THROWAWAY_WORKTREE_PREFIX}{id}");
        let temp = std::env::temp_dir();
        let base = temp.join(format!("c3-wt-parent-id-{}", uuid::Uuid::new_v4().simple()));
        // a look-alike: a directory with the temp directory's own name, somewhere else
        let look = base.join(temp.file_name().unwrap());
        std::fs::create_dir_all(&look).unwrap();
        let why = reusable_worktree_path(&look.join(&name).to_string_lossy()).unwrap_err();
        assert!(
            why.contains("not directly under the system temp directory"),
            "{why}"
        );
        // an alias of the temp directory: the same directory, so reusable - at the BUILT path
        let alias = base.join("alias-of-temp");
        contain::tests::junction_or_symlink(&alias, &temp);
        let built = reusable_worktree_path(&alias.join(&name).to_string_lossy()).unwrap();
        assert_eq!(built, temp.join(&name), "built from the temp directory");
        assert!(!built.starts_with(&alias), "{}", built.display());
        // the decision is made against the temp directory's handle (a stand-in temp here)
        let fake_temp_dir = base.join("a").join("b");
        std::fs::create_dir_all(&fake_temp_dir).unwrap();
        let fake_temp = Root::open_anchor(&fake_temp_dir).unwrap();
        assert_eq!(
            reusable_under(&fake_temp_dir.join(&name).to_string_lossy(), &fake_temp),
            Ok(fake_temp_dir.join(&name))
        );
        assert!(reusable_under(&temp.join(&name).to_string_lossy(), &fake_temp).is_err());
        #[cfg(unix)]
        {
            // `a\b` is ONE directory name on Unix, not the path `a/b`
            let decoy = base.join("a\\b");
            std::fs::create_dir_all(&decoy).unwrap();
            let why = reusable_under(&decoy.join(&name).to_string_lossy(), &fake_temp).unwrap_err();
            assert!(
                why.contains("not directly under the system temp directory"),
                "{why}"
            );
        }
        crate::contain::remove_link(
            &cap_std::fs::Dir::open_ambient_dir(&base, cap_std::ambient_authority()).unwrap(),
            OsStr::new("alias-of-temp"),
            true,
        )
        .unwrap();
        let _ = std::fs::remove_dir_all(&base);
    }

    // (0.2.5 A4, F57-7) THE CLAIM RACE (the reviewer's Verify): the directory claimed for a resumed
    // worktree is replaced by another directory WITH CONTENT at a point between the claim and the
    // check - right after the exclusive create (`worktree-claim`), before `git worktree add`
    // (`worktree-add`) and after it (`worktree-checkout`). Wherever the swap happens the resume is
    // refused (or fails) before anything is staged, the replacement directory is preserved with its
    // content, nothing is staged into it, and git keeps no registration at the path. Where the
    // system refuses the swap (Windows, while C3 holds the claimed directory's handle - from the
    // claim on) the worktree is the claimed directory and is used and removed as usual.
    #[test]
    fn a_claimed_worktree_directory_replaced_before_the_check_refuses_and_is_kept() {
        for point in ["worktree-claim", "worktree-add", "worktree-checkout"] {
            let repo = brief_repo("claim-race");
            let at = c3_shaped_path();
            let name = at.file_name().unwrap().to_string_lossy().to_string();
            let aside = at.with_file_name(format!("{name}.aside"));
            let swapped = std::rc::Rc::new(std::cell::Cell::new(false));
            let step = {
                let (at, aside, swapped) = (at.clone(), aside.clone(), swapped.clone());
                move || {
                    if std::fs::rename(&at, &aside).is_ok() {
                        std::fs::create_dir(&at).unwrap();
                        std::fs::write(at.join("keep.txt"), "not the run's\n").unwrap();
                        swapped.set(true);
                    }
                }
            };
            let guard = interleave_hook::at(point, step);
            let made = ThrowawayWorktree::create_at(&repo, "HEAD", Some(&at));
            drop(guard);
            if point == "worktree-claim" {
                assert!(swapped.get(), "{point}: no handle is held before the open");
            } else if cfg!(windows) {
                assert!(
                    !swapped.get(),
                    "{point}: the claimed directory's handle is held, so it cannot be renamed"
                );
            }
            if swapped.get() {
                match &made {
                    Err(CreateError::Replaced(p, why)) => {
                        assert_eq!(p, &at, "{point}");
                        assert!(point != "worktree-add" && !why.is_empty(), "{point}: {why}");
                    }
                    Err(CreateError::Failed(why)) => {
                        assert_eq!(point, "worktree-add", "{why}");
                        assert!(
                            why.contains("no longer names the directory made for it")
                                && why.contains("nothing there was removed"),
                            "{point}: {why}"
                        );
                    }
                    Err(e) => panic!("{point}: unexpected {e:?}"),
                    Ok(wt) => panic!(
                        "{point}: a replaced directory was used: {}",
                        wt.path().display()
                    ),
                }
                drop(made);
                assert_eq!(
                    tree_names(&at),
                    vec!["keep.txt".to_string()],
                    "{point}: the replacement is untouched, nothing staged into it"
                );
                assert_eq!(
                    std::fs::read_to_string(at.join("keep.txt")).unwrap(),
                    "not the run's\n",
                    "{point}"
                );
                let list = git_capture(&repo, &["worktree", "list", "--porcelain"]).unwrap();
                assert!(!list.contains(&name), "{point}: {list}");
                std::fs::remove_dir_all(&at).unwrap();
                let _ = std::fs::remove_dir_all(&aside);
            } else {
                let wt = made.unwrap_or_else(|e| panic!("{point}: {e:?}"));
                assert_eq!(wt.path(), at.as_path(), "{point}");
                assert!(at.join("committed.md").is_file(), "{point}");
                assert!(!aside.exists(), "{point}");
                drop(wt);
                assert!(std::fs::symlink_metadata(&at).is_err(), "{point}: removed");
            }
            let _ = git_capture(&repo, &["worktree", "prune"]);
            let _ = std::fs::remove_dir_all(repo.parent().unwrap());
        }
    }

    // (0.2.5 D7) `create_at` makes the worktree exactly at a free C3-shaped path (and the path can
    // be used again once the guard removed it); an EXISTING path - a directory with content, a
    // file - is `Occupied`, nothing is made and what sits there is untouched; a path of another
    // shape is a plain failure that makes nothing.
    #[test]
    fn create_at_uses_a_free_path_and_refuses_an_existing_one() {
        let repo = brief_repo("create-at");
        let at = c3_shaped_path();
        let mut wt = ThrowawayWorktree::create_at(&repo, "HEAD", Some(&at)).unwrap();
        assert_eq!(wt.path(), at.as_path());
        assert!(at.join("committed.md").is_file());
        assert!(at.join(".git").is_file(), "a linked worktree");
        assert_eq!(wt.remove(), Ok(()));
        assert!(std::fs::symlink_metadata(&at).is_err(), "removed");
        // the same path again, once free
        let wt2 = ThrowawayWorktree::create_at(&repo, "HEAD", Some(&at)).unwrap();
        assert_eq!(wt2.path(), at.as_path());
        drop(wt2);
        assert!(std::fs::symlink_metadata(&at).is_err());

        // an existing directory (a live run's worktree, a crashed run's leftover)
        std::fs::create_dir_all(&at).unwrap();
        std::fs::write(at.join("left.txt"), "keep me\n").unwrap();
        match ThrowawayWorktree::create_at(&repo, "HEAD", Some(&at)) {
            Err(CreateError::Occupied(p)) => assert_eq!(p, at),
            Err(e) => panic!("expected Occupied, got {e:?}"),
            Ok(wt) => panic!("an existing path was used: {}", wt.path().display()),
        }
        assert_eq!(
            std::fs::read_to_string(at.join("left.txt")).unwrap(),
            "keep me\n"
        );
        let list = git_capture(&repo, &["worktree", "list", "--porcelain"]).unwrap();
        assert!(!list.contains(THROWAWAY_WORKTREE_PREFIX), "{list}");
        std::fs::remove_dir_all(&at).unwrap();
        // an existing (empty) file at the path
        std::fs::write(&at, "").unwrap();
        assert_eq!(
            ThrowawayWorktree::create_at(&repo, "HEAD", Some(&at)).err(),
            Some(CreateError::Occupied(at.clone()))
        );
        assert!(at.is_file(), "the file is untouched");
        std::fs::remove_file(&at).unwrap();

        // another shape: a plain failure, nothing is made there
        let elsewhere = repo.parent().unwrap().join(format!(
            "{THROWAWAY_WORKTREE_PREFIX}{}",
            uuid::Uuid::new_v4().simple()
        ));
        match ThrowawayWorktree::create_at(&repo, "HEAD", Some(&elsewhere)) {
            Err(CreateError::Failed(why)) => {
                assert!(why.contains("cannot be reused"), "{why}")
            }
            other => panic!(
                "expected a failure, got {:?}",
                other.map(|w| w.path.clone())
            ),
        }
        assert!(std::fs::symlink_metadata(&elsewhere).is_err());
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }

    // (0.2.5 D7) When `git worktree add` fails at a claimed path (here: git still has a worktree
    // registered there whose directory is gone), the empty directory the claim made is removed
    // again - nothing is left at the path.
    #[test]
    fn a_failed_checkout_at_a_claimed_path_leaves_nothing_there() {
        let repo = brief_repo("create-at-fails");
        let at = c3_shaped_path();
        git_capture(
            &repo,
            &["worktree", "add", "--detach", &at.to_string_lossy(), "HEAD"],
        )
        .unwrap();
        std::fs::remove_dir_all(&at).unwrap();
        match ThrowawayWorktree::create_at(&repo, "HEAD", Some(&at)) {
            Err(CreateError::Failed(why)) => assert!(!why.contains("left in place"), "{why}"),
            other => panic!(
                "expected a failure, got {:?}",
                other.map(|w| w.path.clone())
            ),
        }
        assert!(
            std::fs::symlink_metadata(&at).is_err(),
            "the claimed directory was removed again"
        );
        let _ = git_capture(&repo, &["worktree", "prune"]);
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }
}
