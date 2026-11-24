//! (0.2.5 D4) Contained file operations: what C3 writes, reads, walks and removes INSIDE a
//! directory it must never leave - a reviewer's throwaway worktree, the repository, a task's
//! handoffs - happens relative to an open directory handle ([`cap_std::fs::Dir`]), never by a path
//! name that was checked earlier and then resolved again (findings F32-1, F24-1, F29-2, F18-5,
//! F27-2, F16-5, F13-9, F28-1: "checked by path name, then used by path name").
//!
//! A root is opened ONCE ([`Root`]) and verified on that handle; every later step is relative to an
//! open handle:
//!
//! * a path below a root is opened ONE COMPONENT AT A TIME, each relative to its parent's handle
//!   and without following it ([`open_child`]): a component that is a link - a symlink, a junction,
//!   any other Windows reparse point (C3's rule: every reparse point counts as a link) - refuses,
//!   even one that would stay inside the root;
//! * a file is read through a handle opened without following it, verified to be a regular file
//!   on that handle ([`read_file_in`]);
//! * a file is written to a NEW temporary name in its directory (an exclusive create relative to the
//!   directory's handle) and published by a rename relative to the same handle, replacing nothing but
//!   a regular file; whatever this call created is removed through the same handle on failure, and
//!   a failed removal is reported ([`publish`]);
//! * (F57-10, F02-2) a file a person may edit by hand at the same time is published by an atomic
//!   EXCHANGE of the temporary file and the file instead ([`publish_exchange`]): the version it
//!   displaced is read back and kept next to the file when it is not the one the edit was computed
//!   from - an edit saved during the write is never lost;
//! * a tree is walked without ever descending into a link ([`walk_links`]) and removed through the
//!   handles ([`remove_contents`], [`remove_root`]).
//!
//! What holds it on each platform (proved by `cap_std_refuses_a_link_as_a_path_component`):
//! on Linux and macOS cap-std's operations are the `*at` system calls on the directory's file
//! descriptor (`openat` with `O_NOFOLLOW`, `mkdirat`, `renameat`, `unlinkat`), so a component that
//! is renamed or replaced after it was opened cannot redirect them - they complete in the directory
//! that was opened. On Windows a file is created and opened relative to the directory handle
//! (`NtCreateFile` with a root directory); cap-std opens every directory WITHOUT
//! `FILE_SHARE_DELETE`, so while C3 holds a directory's handle neither that directory nor any of its
//! ancestors can be renamed or deleted (the system refuses with a sharing violation / access
//! denied) - the path the remaining operations (rename, remove, create a directory) are built from
//! cannot name another directory meanwhile. A junction or directory symlink in a component is
//! refused there by the no-follow open (`ERROR_STOPPED_ON_SYMLINK`), and cap-std refuses to resolve
//! a junction even when asked to follow (its target is absolute: "a path led outside of the
//! filesystem").
//!
//! The race tests drive [`race_point`]: a test-only hook ([`RACE_HOOK`], `c3_core::test_hooks`)
//! that swaps a directory for a link at a named point between validation and use.

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Component, Path, PathBuf};

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt, OpenOptionsMaybeDirExt};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, File, Metadata, OpenOptions};

/// (0.2.5 D4) The race hook (test mode, or a thread hook of an in-process test): one line per swap,
/// `<point>|<directory>|<link target>` - when the operation reaches `<point>`, `<directory>` is
/// renamed aside (to `<name>.c3-race-orig`) and a link to `<link target>` is made in its place (a
/// junction on Windows, a symlink elsewhere). On Windows the rename fails while C3 holds the
/// directory's handle, which is the protection; the test reads which of the two happened.
/// (F57-10, F02-2) A line `<point>|write:<file>|<text>` instead writes `<text>` to `<file>` in
/// place when the operation reaches `<point>` - an editor's save landing at that moment.
pub(crate) const RACE_HOOK: &str = "CODEX_CONSULT_TEST_CONTAIN_RACE";

/// Windows' `FILE_ATTRIBUTE_REPARSE_POINT`.
#[cfg(windows)]
const ATTR_REPARSE: u32 = 0x400;
/// Windows' `FILE_ATTRIBUTE_DIRECTORY`.
#[cfg(windows)]
const ATTR_DIRECTORY: u32 = 0x10;

/// What an entry is, read without following it (metadata through its parent's handle).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A link: a symlink, or on Windows any reparse point. `symlink`: Rust's `is_symlink` (on
    /// Windows a name-surrogate reparse point - a symlink or a junction); `dir`: it carries the
    /// directory attribute (Windows: removed with `remove_dir`).
    Link {
        symlink: bool,
        dir: bool,
    },
    Dir,
    File,
    Other,
}

/// [`Kind`] of metadata read without following the entry.
pub(crate) fn kind_of(m: &Metadata) -> Kind {
    let symlink = m.file_type().is_symlink();
    #[cfg(windows)]
    let (reparse, dir_attr) = {
        use cap_std::fs::MetadataExt;
        let a = m.file_attributes();
        (a & ATTR_REPARSE != 0, a & ATTR_DIRECTORY != 0)
    };
    #[cfg(not(windows))]
    let (reparse, dir_attr) = (false, false);
    if symlink || reparse {
        Kind::Link {
            symlink,
            dir: dir_attr,
        }
    } else if m.is_dir() {
        Kind::Dir
    } else if m.is_file() {
        Kind::File
    } else {
        Kind::Other
    }
}

/// The entry `name` of `dir`, read without following it; `None` when it is not there.
pub(crate) fn entry(dir: &Dir, name: &OsStr) -> io::Result<Option<Kind>> {
    match dir.symlink_metadata(name) {
        Ok(m) => Ok(Some(kind_of(&m))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// The normal components of a relative path (`.` dropped); `None` for a path with a root, a drive
/// prefix or a `..` - such a path is never resolved below a root.
pub(crate) fn components(rel: &Path) -> Option<Vec<&OsStr>> {
    let mut v = Vec::new();
    for c in rel.components() {
        match c {
            Component::Normal(n) => v.push(n),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(v)
}

/// Why one step below a root did not give what was asked.
#[derive(Debug)]
pub(crate) enum Step {
    /// The entry is a link (never followed). `symlink` as in [`Kind::Link`].
    Link { symlink: bool },
    /// A directory was needed and the entry is something else.
    NotDir,
    /// A regular file was needed and the entry is something else.
    NotFile,
    /// The entry is not there.
    Missing,
    /// The path is not a plain relative path (a root, a prefix, a `..`).
    BadPath,
    /// The directory could not be created.
    Create(io::Error),
    /// The entry could not be opened or read.
    Io(io::Error),
}

impl std::fmt::Display for Step {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Step::Link { .. } => write!(f, "is a link"),
            Step::NotDir => write!(f, "is not a directory"),
            Step::NotFile => write!(f, "is not a regular file"),
            Step::Missing => write!(f, "is not there"),
            Step::BadPath => write!(f, "is not a path inside the root"),
            Step::Create(e) => write!(f, "could not be created ({e})"),
            Step::Io(e) => write!(f, "cannot be read ({e})"),
        }
    }
}

/// A [`Step`] at the root-relative path `at` (the components up to and including the one that
/// failed).
#[derive(Debug)]
pub(crate) struct StepError {
    pub(crate) at: PathBuf,
    pub(crate) why: Step,
}

/// Why `name` in `dir` failed to open as asked (`e`), read without following it.
fn classify(dir: &Dir, name: &OsStr, e: io::Error, want_dir: bool) -> Step {
    match entry(dir, name) {
        Ok(None) => Step::Missing,
        Ok(Some(Kind::Link { symlink, .. })) => Step::Link { symlink },
        Ok(Some(Kind::Dir)) if want_dir => Step::Io(e),
        Ok(Some(Kind::File)) if !want_dir => Step::Io(e),
        Ok(Some(_)) if want_dir => Step::NotDir,
        Ok(Some(_)) => Step::NotFile,
        Err(_) => Step::Io(e),
    }
}

/// A directory handle that is a real directory (not a link), checked on the handle itself.
fn verified_dir(d: Dir) -> Result<Dir, Step> {
    let m = d.dir_metadata().map_err(Step::Io)?;
    match kind_of(&m) {
        Kind::Dir => Ok(d),
        Kind::Link { symlink, .. } => Err(Step::Link { symlink }),
        _ => Err(Step::NotDir),
    }
}

/// Open the directory `name` of `dir` WITHOUT following it (a link, on Windows any reparse point,
/// refuses), creating it first when `create` and it is not there. The handle is checked to be a
/// real directory.
pub(crate) fn open_child(dir: &Dir, name: &OsStr, create: bool) -> Result<Dir, Step> {
    match dir.open_dir_nofollow(name) {
        Ok(d) => verified_dir(d),
        Err(e) => match classify(dir, name, e, true) {
            Step::Missing if create => {
                match dir.create_dir(name) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(Step::Create(e)),
                }
                match dir.open_dir_nofollow(name) {
                    Ok(d) => verified_dir(d),
                    Err(e) => Err(classify(dir, name, e, true)),
                }
            }
            other => Err(other),
        },
    }
}

/// The options of a read-only open that does not follow the final component. (0.2.5 R7b, F44-3)
/// On Unix the open does not block either (`O_NONBLOCK`): a FIFO swapped in after the kind check
/// opens at once instead of waiting for a writer, and the handle check refuses it before any read
/// (on a regular file the flag changes nothing). Windows needs nothing: a named pipe lives in its
/// own namespace, never as an entry of a directory.
fn nofollow_read() -> OpenOptions {
    let mut o = OpenOptions::new();
    o.read(true).follow(FollowSymlinks::No);
    #[cfg(unix)]
    {
        use cap_fs_ext::OpenOptionsSyncExt;
        o.nonblock(true);
    }
    o
}

/// Open the file `name` of `dir` for reading WITHOUT following it; the handle is checked to be a
/// regular file (not a link, not a reparse point, not a directory, device or FIFO). A name that is
/// not a regular file when read without following it is refused BEFORE it is opened; an entry
/// swapped in between is opened without blocking and refused on its handle ([`open_checked`]).
pub(crate) fn open_file_in(dir: &Dir, name: &OsStr) -> Result<File, Step> {
    match entry(dir, name) {
        Ok(None) => return Err(Step::Missing),
        Ok(Some(Kind::Link { symlink, .. })) => return Err(Step::Link { symlink }),
        Ok(Some(Kind::File)) => {}
        Ok(Some(_)) => return Err(Step::NotFile),
        Err(e) => return Err(Step::Io(e)),
    }
    open_checked(dir, name)
}

/// The open of [`open_file_in`] after its kind check: without following the name and without
/// blocking ([`nofollow_read`]), then the HANDLE's metadata decides - only a regular file is
/// returned, so nothing else (a FIFO, a device, a link) is ever read.
fn open_checked(dir: &Dir, name: &OsStr) -> Result<File, Step> {
    let f = dir
        .open_with(name, &nofollow_read())
        .map_err(|e| classify(dir, name, e, false))?;
    let m = f.metadata().map_err(Step::Io)?;
    match kind_of(&m) {
        Kind::File => Ok(f),
        Kind::Link { symlink, .. } => Err(Step::Link { symlink }),
        _ => Err(Step::NotFile),
    }
}

/// The bytes of the regular file `name` of `dir` ([`open_file_in`]).
pub(crate) fn read_file_in(dir: &Dir, name: &OsStr) -> Result<Vec<u8>, Step> {
    use std::io::Read;
    let mut f = open_file_in(dir, name)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(Step::Io)?;
    Ok(buf)
}

/// The names in `dir` (its own handle; `.` and `..` are never listed), ordinal-sorted.
pub(crate) fn list(dir: &Dir) -> io::Result<Vec<OsString>> {
    let mut names = Vec::new();
    for e in dir.entries()? {
        names.push(e?.file_name());
    }
    names.sort();
    Ok(names)
}

/// (0.2.5 D5) Remove from `dir`, through its handle, every REGULAR file whose name `is_temp`
/// accepts and that has not changed for `stale_secs` seconds or more: the write-temp debris a writer
/// killed between its write and its rename leaves, which nothing else ever removes. Each entry is
/// judged on metadata read without following it ([`kind_of`]: a link - on Windows any reparse
/// point - a directory or anything else is never touched) and removed relative to the same handle.
/// Best effort: every failure is ignored, so the caller's own write never fails for it.
pub(crate) fn sweep_stale_temps(dir: &Dir, is_temp: impl Fn(&str) -> bool, stale_secs: u64) {
    let Ok(entries) = dir.entries() else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        if !name.to_str().is_some_and(&is_temp) {
            continue;
        }
        let stale = dir
            .symlink_metadata(&name)
            .ok()
            .filter(|m| kind_of(m) == Kind::File)
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.into_std().elapsed().ok())
            .is_some_and(|age| age.as_secs() >= stale_secs);
        if stale {
            let _ = dir.remove_file(&name);
        }
    }
}

/// Why a root could not be opened.
#[derive(Debug)]
pub(crate) enum RootError {
    /// The root itself is a link (a symlink, a junction, any reparse point).
    Link,
    NotDir,
    Missing,
    Io(io::Error),
}

impl RootError {
    /// The words after the root's name: `is a link`, `is not a directory`, ...
    pub(crate) fn words(&self) -> String {
        match self {
            RootError::Link => "is a link".to_string(),
            RootError::NotDir => "is not a directory".to_string(),
            RootError::Missing => "is not there".to_string(),
            RootError::Io(e) => format!("cannot be read ({e})"),
        }
    }
}

/// Why `path` could not be opened as a root (`e`), read without following it.
fn classify_root(path: &Path, e: io::Error) -> RootError {
    match std::fs::symlink_metadata(path) {
        Err(m) if m.kind() == io::ErrorKind::NotFound => RootError::Missing,
        Err(_) => RootError::Io(e),
        Ok(m) if crate::engines::worktree::is_link(&m) || m.file_type().is_symlink() => {
            RootError::Link
        }
        Ok(m) if !m.is_dir() => RootError::NotDir,
        Ok(_) => RootError::Io(e),
    }
}

/// A file's identity on its volume: (device / volume serial, inode / file index), read from an
/// open handle.
pub(crate) type FileId = (u64, u64);

fn identity(m: &Metadata) -> FileId {
    (
        cap_fs_ext::MetadataExt::dev(m),
        cap_fs_ext::MetadataExt::ino(m),
    )
}

/// A directory opened ONCE and verified on its handle; everything below it is reached through
/// handles ([`open_child`]). `path` is kept for messages (and the root's own removal,
/// [`remove_root`]); it is never used to reach an entry.
#[derive(Debug)]
pub(crate) struct Root {
    dir: Dir,
    path: PathBuf,
    id: FileId,
}

impl Root {
    /// Open `path` as a root that must itself be a real directory: it is opened WITHOUT following
    /// it, and the handle is checked (not a link, not any reparse point, a directory) - a
    /// throwaway worktree C3 made.
    pub(crate) fn open(path: &Path) -> Result<Root, RootError> {
        let mut o = OpenOptions::new();
        o.read(true).follow(FollowSymlinks::No).maybe_dir(true);
        let f = File::open_ambient_with(path, &o, ambient_authority())
            .map_err(|e| classify_root(path, e))?;
        let m = f.metadata().map_err(RootError::Io)?;
        match kind_of(&m) {
            Kind::Dir => {}
            Kind::Link { .. } => return Err(RootError::Link),
            _ => return Err(RootError::NotDir),
        }
        let dir = Dir::from_std_file(f.into_std());
        Ok(Root {
            dir,
            path: path.to_path_buf(),
            id: identity(&m),
        })
    }

    /// Open `path` as an ANCHOR: the name is followed to its directory (the repository root, a
    /// task's handoffs directory - where they live is not C3's to judge), and only what lies BELOW
    /// it is contained. The handle is checked to be a directory.
    pub(crate) fn open_anchor(path: &Path) -> Result<Root, RootError> {
        let dir =
            Dir::open_ambient_dir(path, ambient_authority()).map_err(
                |e| match std::fs::metadata(path) {
                    Err(m) if m.kind() == io::ErrorKind::NotFound => RootError::Missing,
                    Ok(m) if !m.is_dir() => RootError::NotDir,
                    _ => RootError::Io(e),
                },
            )?;
        let m = dir.dir_metadata().map_err(RootError::Io)?;
        if !m.is_dir() {
            return Err(RootError::NotDir);
        }
        Ok(Root {
            dir,
            path: path.to_path_buf(),
            id: identity(&m),
        })
    }

    /// The path the root was opened by (for messages).
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The root directory's identity, read on its handle when it was opened.
    pub(crate) fn id(&self) -> FileId {
        self.id
    }

    /// The root's handle.
    pub(crate) fn dir(&self) -> &Dir {
        &self.dir
    }

    /// Open the directory `rel` below the root one component at a time ([`open_child`]; created
    /// when `create`). An empty `rel` is the root itself.
    pub(crate) fn open_dir(&self, rel: &Path, create: bool) -> Result<Dir, StepError> {
        let comps = components(rel).ok_or_else(|| StepError {
            at: rel.to_path_buf(),
            why: Step::BadPath,
        })?;
        let mut cur = self.dir.try_clone().map_err(|e| StepError {
            at: PathBuf::new(),
            why: Step::Io(e),
        })?;
        let mut at = PathBuf::new();
        for c in comps {
            at.push(c);
            cur = open_child(&cur, c, create).map_err(|why| StepError {
                at: at.clone(),
                why,
            })?;
        }
        Ok(cur)
    }

    /// The parent directory of the file `rel` (opened as [`Root::open_dir`]) and the file's name.
    pub(crate) fn open_parent(
        &self,
        rel: &Path,
        create: bool,
    ) -> Result<(Dir, OsString), StepError> {
        let bad = || StepError {
            at: rel.to_path_buf(),
            why: Step::BadPath,
        };
        let comps = components(rel).ok_or_else(bad)?;
        let (leaf, parents) = comps.split_last().ok_or_else(bad)?;
        let parent: PathBuf = parents.iter().collect();
        let dir = self.open_dir(&parent, create)?;
        Ok((dir, leaf.to_os_string()))
    }

    /// What `rel` is, no component followed: `Ok(None)` when it (or a directory on its way) is not
    /// there; a link on the way is an `Err` at that component.
    pub(crate) fn kind(&self, rel: &Path) -> Result<Option<Kind>, StepError> {
        let (dir, leaf) = match self.open_parent(rel, false) {
            Ok(v) => v,
            Err(StepError {
                why: Step::Missing, ..
            }) => return Ok(None),
            Err(e) => return Err(e),
        };
        entry(&dir, &leaf).map_err(|e| StepError {
            at: rel.to_path_buf(),
            why: Step::Io(e),
        })
    }

    /// The bytes of the regular file `rel` below the root: no component and not the file itself is
    /// followed ([`read_file_in`]).
    pub(crate) fn read(&self, rel: &Path) -> Result<Vec<u8>, StepError> {
        race_point("read-walk");
        let (dir, leaf) = self.open_parent(rel, false)?;
        race_point("read-open");
        read_file_in(&dir, &leaf).map_err(|why| StepError {
            at: rel.to_path_buf(),
            why,
        })
    }
}

/// The bytes of the regular file at `path`, its directory opened as an anchor ([`Root::open_anchor`])
/// and the file itself opened without following it. `Err` is the reason alone (the caller names
/// the path).
pub(crate) fn read_path(path: &Path) -> Result<Vec<u8>, String> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err("it names no file".to_string());
    };
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    let root = Root::open_anchor(parent).map_err(|e| format!("its directory {}", e.words()))?;
    read_file_in(root.dir(), name).map_err(|why| format!("it {why}"))
}

/// When [`publish`] runs its check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    /// Before the temporary file is created.
    BeforeCreate,
    /// After it was written and synced, right before the rename.
    BeforeRename,
}

/// One publication: `bytes` written to the NEW file `tmp` of `dir`, then renamed onto `leaf`.
pub(crate) struct Publish<'a> {
    pub(crate) dir: &'a Dir,
    pub(crate) leaf: &'a OsStr,
    pub(crate) tmp: &'a OsStr,
    pub(crate) bytes: &'a [u8],
    /// Permission bits the new file takes before it is published (the file it replaces keeps
    /// them); `None` leaves the defaults.
    pub(crate) permissions: Option<cap_std::fs::Permissions>,
    /// The caller's check of the destination, before the create and again right before the rename.
    pub(crate) check: &'a dyn Fn(Stage) -> Result<(), String>,
    /// The race-point prefix (`<race>-create`, `<race>-publish`).
    pub(crate) race: &'a str,
}

/// What failed in [`publish`].
#[derive(Debug)]
pub(crate) enum PublishError {
    /// The caller's check refused.
    Check(String),
    /// The temporary file could not be created.
    Create(io::Error),
    /// It could not be written or synced.
    Write(io::Error),
    /// It could not be renamed onto the destination.
    Rename(io::Error),
}

/// A failed [`publish`]: what failed and, when the temporary file could not be removed again, why.
#[derive(Debug)]
pub(crate) struct PublishFailure {
    pub(crate) error: PublishError,
    /// The temporary file's removal failed (it is still there).
    pub(crate) cleanup: Option<io::Error>,
    pub(crate) tmp: OsString,
}

impl PublishFailure {
    /// `msg` with the failed cleanup appended, when there was one.
    pub(crate) fn with_cleanup(&self, msg: String) -> String {
        match &self.cleanup {
            None => msg,
            Some(e) => format!(
                "{msg}; its temporary file {} could not be removed ({e})",
                self.tmp.to_string_lossy()
            ),
        }
    }
}

/// Write `p.bytes` to the NEW file `p.tmp` of `p.dir` (an exclusive create relative to the
/// directory's handle: nothing can be planted there, no link followed), sync it, run the caller's
/// check again, then rename it onto `p.leaf` relative to the same handle. A rename replaces only
/// the directory entry - a link at the destination is replaced, never written through (the caller's
/// check refuses one anyway). On any failure the temporary file is removed through the same handle;
/// a removal that fails is reported in [`PublishFailure::cleanup`].
pub(crate) fn publish(p: &Publish<'_>) -> Result<(), PublishFailure> {
    stage_temp(p)?;
    race_point(&format!("{}-publish", p.race));
    p.dir
        .rename(p.tmp, p.dir, p.leaf)
        .map_err(|e| failure(p, PublishError::Rename(e), remove_temp(p)))
}

/// A [`PublishFailure`] of `p`.
fn failure(p: &Publish<'_>, error: PublishError, cleanup: Option<io::Error>) -> PublishFailure {
    PublishFailure {
        error,
        cleanup,
        tmp: p.tmp.to_os_string(),
    }
}

/// Remove `p.tmp` through the directory's handle; `Some` is a removal that failed (a temporary file
/// that is not there is no failure).
fn remove_temp(p: &Publish<'_>) -> Option<io::Error> {
    match p.dir.remove_file(p.tmp) {
        Ok(()) => None,
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => Some(e),
    }
}

/// The first half of [`publish`] and [`publish_exchange`]: the caller's check, the NEW temporary
/// file created exclusively relative to the directory's handle, written and synced (its permission
/// bits set), and the caller's check again. On any failure the temporary file is removed.
fn stage_temp(p: &Publish<'_>) -> Result<(), PublishFailure> {
    (p.check)(Stage::BeforeCreate).map_err(|why| failure(p, PublishError::Check(why), None))?;
    race_point(&format!("{}-create", p.race));
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    let mut f = p
        .dir
        .open_with(p.tmp, &o)
        .map_err(|e| failure(p, PublishError::Create(e), None))?;
    let written = (|| -> io::Result<()> {
        write_hook(&mut f, p.bytes)?;
        f.sync_all()?;
        if let Some(perm) = &p.permissions {
            f.set_permissions(perm.clone())?;
        }
        Ok(())
    })();
    drop(f);
    if let Err(e) = written {
        return Err(failure(p, PublishError::Write(e), remove_temp(p)));
    }
    if let Err(why) = (p.check)(Stage::BeforeRename) {
        return Err(failure(p, PublishError::Check(why), remove_temp(p)));
    }
    Ok(())
}

/// (F57-10, F02-2) The test hook that makes [`publish_exchange`] behave as on a system without an
/// atomic exchange (`CODEX_CONSULT_TEST_CONTAIN_NO_EXCHANGE`, any value): its compare-then-rename
/// fallback is then proved.
pub(crate) const NO_EXCHANGE_HOOK: &str = "CODEX_CONSULT_TEST_CONTAIN_NO_EXCHANGE";

/// How [`publish_exchange`] put the new bytes in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Publication {
    /// Atomically exchanged with the version it displaced: Windows `ReplaceFileW`, Linux
    /// `renameat2(RENAME_EXCHANGE)`, macOS `renamex_np(RENAME_SWAP)`.
    Exchange,
    /// The file was not there: published without replacing anything (a hard link to the new
    /// name, which fails when a file appeared meanwhile - then the exchange is tried again).
    Created,
    /// No atomic exchange on this system or file system: published by [`publish`]'s
    /// compare-then-rename, so an edit saved between the last comparison and the rename is lost -
    /// the limit that remains there. Why.
    Rename(String),
}

/// A displaced version [`publish_exchange`] kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Kept {
    /// Its name in the directory (`<file>.displaced-<UTC time>`, [`displaced_name`]).
    pub(crate) name: OsString,
    /// `None`: it differs from the version the edit was computed from - an edit saved during the
    /// write. `Some`: it could not be proved to be that version, or could not be removed - why.
    pub(crate) unproven: Option<String>,
}

/// What [`publish_exchange`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Exchanged {
    pub(crate) how: Publication,
    /// The displaced version, when it was not the basis: kept next to the file.
    pub(crate) kept: Option<Kept>,
    /// The temporary name could not be removed after a [`Publication::Created`] (it is a second
    /// name of the new file): why.
    pub(crate) cleanup: Option<String>,
}

impl Exchanged {
    /// The line a caller prints - loudly - about a write of `file` whose displaced version is kept
    /// at `kept_at` (the caller's spelling of the [`Kept::name`] path), or that was published by
    /// rename; `None` when there is nothing to say (an exchange that displaced the basis itself, a
    /// first write).
    pub(crate) fn notice(&self, file: &str, kept_at: impl Fn(&OsStr) -> String) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        if let Some(k) = &self.kept {
            let at = kept_at(&k.name);
            parts.push(match &k.unproven {
                None => format!(
                    "a hand edit of {file} landed during C3's write: it is kept at {at}; the file now holds C3's version"
                ),
                Some(why) => format!(
                    "the version of {file} C3's write displaced is kept at {at} ({why}); the file now holds C3's version"
                ),
            });
        }
        if let Publication::Rename(why) = &self.how {
            parts.push(format!(
                "{file} was replaced by rename, not by an atomic exchange ({why}): an edit saved during the write would not be kept"
            ));
        }
        if let Some(why) = &self.cleanup {
            parts.push(format!("{file}: {why}"));
        }
        (!parts.is_empty()).then(|| parts.join("; "))
    }
}

/// (F57-10, F02-2) [`publish`] for a file a person may edit by hand at the same time (the roster,
/// a repository's agent and rule files): the version the write DISPLACES is never lost. The new
/// bytes go to the NEW temporary file `p.tmp` and the caller's check (the comparison with the basis
/// the edit was computed from) runs as in [`publish`]; then the temporary file and `p.leaf` are
/// ATOMICALLY EXCHANGED ([`Publication::Exchange`]) - there is no moment between a comparison and
/// the publication in which a save could be overwritten unseen. The displaced version is read back
/// through the directory's handle: when it is byte for byte `basis` it is removed; otherwise - an
/// edit saved after the last comparison - it is KEPT next to the file under
/// `<file>.displaced-<UTC time>` ([`displaced_name`]) and returned ([`Exchanged::kept`]) for the
/// caller to report. Not there at all: published without replacing anything
/// ([`Publication::Created`]). No exchange on this system or file system (other Unix targets,
/// `EINVAL` / `ENOSYS` / `ENOTSUP`, `ReplaceFileW` unsupported): [`publish`]'s compare-then-rename,
/// said in the result ([`Publication::Rename`]).
///
/// On Windows the exchange is `ReplaceFileW` on the paths of the directory the handle holds
/// (`GetFinalPathNameByHandleW`; while the handle is open the directory cannot be renamed, the same
/// ground the module's other Windows operations stand on), the displaced version landing at
/// `<tmp>.displaced`; a sharing violation (an editor holding the file) is retried up to 8 times,
/// 250 ms apart, as `c3_core::store::write_text_atomic` does. On Linux and macOS it is
/// `renameat_with(RENAME_EXCHANGE)` relative to the directory's descriptor, the displaced version
/// landing at `p.tmp`.
pub(crate) fn publish_exchange(p: &Publish<'_>, basis: &[u8]) -> Result<Exchanged, PublishFailure> {
    stage_temp(p)?;
    race_point(&format!("{}-publish", p.race));
    let fallback = |why: String| -> Result<Exchanged, PublishFailure> {
        p.dir
            .rename(p.tmp, p.dir, p.leaf)
            .map_err(|e| failure(p, PublishError::Rename(e), remove_temp(p)))?;
        Ok(Exchanged {
            how: Publication::Rename(why),
            kept: None,
            cleanup: None,
        })
    };
    if c3_core::test_hooks::hook(NO_EXCHANGE_HOOK).is_some() {
        return fallback("the exchange is switched off by a test hook".to_string());
    }
    let mut backup = p.tmp.to_os_string();
    backup.push(".displaced");
    // a file that disappears and reappears between the tries is given three rounds
    for _ in 0..3 {
        match exchange(p.dir, p.tmp, p.leaf, &backup) {
            Exchange::Swapped(at) => {
                return Ok(Exchanged {
                    how: Publication::Exchange,
                    kept: settle_displaced(p.dir, &at, p.leaf, Some(basis)),
                    cleanup: None,
                })
            }
            Exchange::Missing => match p.dir.hard_link(p.tmp, p.dir, p.leaf) {
                Ok(()) => {
                    let cleanup = remove_temp(p).map(|e| {
                        format!(
                            "its temporary name {} could not be removed ({e})",
                            p.tmp.to_string_lossy()
                        )
                    });
                    return Ok(Exchanged {
                        how: Publication::Created,
                        kept: None,
                        cleanup,
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return fallback(format!(
                        "the file was not there and no hard link could be made ({e})"
                    ))
                }
            },
            Exchange::Unsupported(why) => return fallback(why),
            Exchange::Failed(e, left) => {
                let kept = left.and_then(|at| settle_displaced(p.dir, &at, p.leaf, None));
                let e = match kept.map(|k| k.name) {
                    Some(name) => io::Error::new(
                        e.kind(),
                        format!(
                            "{e}; the version it displaced is kept as {}",
                            name.to_string_lossy()
                        ),
                    ),
                    None => e,
                };
                return Err(failure(p, PublishError::Rename(e), remove_temp(p)));
            }
        }
    }
    Err(failure(
        p,
        PublishError::Rename(io::Error::other(
            "the file kept disappearing and reappearing during the write",
        )),
        remove_temp(p),
    ))
}

/// The displaced version at `at` after an exchange: removed when it reads back (through the
/// directory's handle, not followed) as exactly `basis`; otherwise renamed to
/// `<leaf>.displaced-<UTC time>` ([`displaced_name`]) and returned. (A failed publication passes no
/// basis: whatever it left is kept.)
fn settle_displaced(dir: &Dir, at: &OsStr, leaf: &OsStr, basis: Option<&[u8]>) -> Option<Kept> {
    let unproven = match read_file_in(dir, at) {
        Ok(b) if basis == Some(b.as_slice()) => match dir.remove_file(at) {
            Ok(()) => return None,
            Err(e) => Some(format!(
                "it is the version the edit was computed from, but could not be removed ({e})"
            )),
        },
        Ok(_) => None,
        Err(why) => Some(format!("it could not be read back: it {why}")),
    };
    let name = displaced_name(dir, leaf);
    match dir.rename(at, dir, &name) {
        Ok(()) => Some(Kept { name, unproven }),
        Err(e) => Some(Kept {
            name: at.to_os_string(),
            unproven: Some(format!(
                "{}it could not be renamed to {} ({e})",
                unproven.map(|u| format!("{u}; ")).unwrap_or_default(),
                name.to_string_lossy()
            )),
        }),
    }
}

/// A free name for a displaced version of `leaf` in `dir`: `<leaf>.displaced-<yyyymmddTHHMMSS.mmm>Z`
/// (UTC), with `-2`, `-3` ... appended while such an entry exists. (Only C3 makes such names; two
/// C3 writes of one file in the same millisecond are what the suffix is for.)
pub(crate) fn displaced_name(dir: &Dir, leaf: &OsStr) -> OsString {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let base = format!("{}.displaced-{stamp}", leaf.to_string_lossy());
    let mut name = OsString::from(&base);
    let mut n = 1;
    while !matches!(entry(dir, &name), Ok(None)) && n < 1000 {
        n += 1;
        name = OsString::from(format!("{base}-{n}"));
    }
    name
}

/// What one exchange attempt did.
#[cfg_attr(
    not(any(
        windows,
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple"
    )),
    allow(dead_code)
)]
enum Exchange {
    /// Exchanged: the displaced version is now at this name of the directory.
    Swapped(OsString),
    /// The file to replace is not there.
    Missing,
    /// No atomic exchange on this system or file system: why.
    Unsupported(String),
    /// It failed; a displaced version it left at this name (Windows), if any.
    Failed(io::Error, Option<OsString>),
}

/// (Windows) `ReplaceFileW`: `tmp` takes the place of `leaf`, whose version lands at `backup`.
#[cfg(windows)]
fn exchange(dir: &Dir, tmp: &OsStr, leaf: &OsStr, backup: &OsStr) -> Exchange {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, REPLACEFILE_IGNORE_ACL_ERRORS, REPLACEFILE_IGNORE_MERGE_ERRORS,
    };
    const ERROR_INVALID_FUNCTION: i32 = 1;
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_NOT_SUPPORTED: i32 = 50;
    const ERROR_UNABLE_TO_REMOVE_REPLACED: i32 = 1175;
    const ERROR_UNABLE_TO_MOVE_REPLACEMENT_2: i32 = 1177;
    const ATTEMPTS: u32 = 8;
    let base = match dir_path(dir) {
        Ok(b) => b,
        Err(e) => return Exchange::Failed(e, None),
    };
    let wide = |name: &OsStr| -> Vec<u16> {
        base.join(name)
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let (target, replacement, displaced) = (wide(leaf), wide(tmp), wide(backup));
    let mut attempt = 1;
    loop {
        // SAFETY: three NUL-terminated UTF-16 strings that outlive the call; the two reserved
        // arguments are null, as the API requires.
        let ok = unsafe {
            ReplaceFileW(
                target.as_ptr(),
                replacement.as_ptr(),
                displaced.as_ptr(),
                REPLACEFILE_IGNORE_MERGE_ERRORS | REPLACEFILE_IGNORE_ACL_ERRORS,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if ok != 0 {
            return Exchange::Swapped(backup.to_os_string());
        }
        let e = io::Error::last_os_error();
        match e.raw_os_error() {
            Some(ERROR_FILE_NOT_FOUND) => return Exchange::Missing,
            Some(ERROR_INVALID_FUNCTION | ERROR_NOT_SUPPORTED) => {
                return Exchange::Unsupported(format!("ReplaceFileW is not supported here ({e})"))
            }
            Some(
                ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION | ERROR_UNABLE_TO_REMOVE_REPLACED,
            ) if attempt < ATTEMPTS => {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            code => {
                if code == Some(ERROR_UNABLE_TO_MOVE_REPLACEMENT_2) {
                    // the file was moved to `backup` but the new one not into its place: put it
                    // back (no replace - should a file have appeared there, both stay)
                    // SAFETY: as above.
                    unsafe { MoveFileExW(displaced.as_ptr(), target.as_ptr(), 0) };
                }
                let left = matches!(entry(dir, backup), Ok(Some(_))).then(|| backup.to_os_string());
                return Exchange::Failed(e, left);
            }
        }
    }
}

/// (Windows) The path of the directory `dir`'s handle holds (`GetFinalPathNameByHandleW`, the
/// `\\?\` form).
#[cfg(windows)]
fn dir_path(dir: &Dir) -> io::Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFinalPathNameByHandleW, FILE_NAME_NORMALIZED, VOLUME_NAME_DOS,
    };
    let mut buf: Vec<u16> = vec![0; 512];
    loop {
        // SAFETY: the handle is the open directory's, valid for the call; `buf` holds `buf.len()`
        // UTF-16 units.
        let n = unsafe {
            GetFinalPathNameByHandleW(
                dir.as_raw_handle(),
                buf.as_mut_ptr(),
                buf.len() as u32,
                FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
            )
        } as usize;
        if n == 0 {
            return Err(io::Error::last_os_error());
        }
        if n < buf.len() {
            buf.truncate(n);
            return Ok(PathBuf::from(OsString::from_wide(&buf)));
        }
        buf.resize(n + 1, 0);
    }
}

/// (Linux, Android, Apple) `renameat2(RENAME_EXCHANGE)` / `renamex_np(RENAME_SWAP)` relative to the
/// directory's descriptor: `tmp` and `leaf` swap, the displaced version lands at `tmp`.
#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
fn exchange(dir: &Dir, tmp: &OsStr, leaf: &OsStr, _backup: &OsStr) -> Exchange {
    use rustix::fs::{renameat_with, RenameFlags};
    use rustix::io::Errno;
    match renameat_with(dir, tmp, dir, leaf, RenameFlags::EXCHANGE) {
        Ok(()) => Exchange::Swapped(tmp.to_os_string()),
        Err(e) if e == Errno::NOENT => Exchange::Missing,
        Err(e)
            if e == Errno::INVAL
                || e == Errno::NOSYS
                || e == Errno::NOTSUP
                || e == Errno::OPNOTSUPP =>
        {
            Exchange::Unsupported(format!(
                "the file system refuses an atomic exchange ({})",
                io::Error::from(e)
            ))
        }
        Err(e) => Exchange::Failed(io::Error::from(e), None),
    }
}

/// (Any other system) No atomic exchange.
#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple"
)))]
fn exchange(_dir: &Dir, _tmp: &OsStr, _leaf: &OsStr, _backup: &OsStr) -> Exchange {
    Exchange::Unsupported("this system has no atomic exchange of two names".to_string())
}

/// (0.2.5 D4) The test hook that makes a write of a [`publish`] or a patch fail after half the bytes
/// (`CODEX_CONSULT_TEST_CONTAIN_WRITE_FAIL`, any value): the error paths' cleanup is then proved.
pub(crate) const WRITE_FAIL_HOOK: &str = "CODEX_CONSULT_TEST_CONTAIN_WRITE_FAIL";

/// `f.write_all(bytes)`, or - under [`WRITE_FAIL_HOOK`] - half of them and an injected error.
pub(crate) fn write_hook(f: &mut File, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    if c3_core::test_hooks::hook(WRITE_FAIL_HOOK).is_some() {
        f.write_all(&bytes[..bytes.len() / 2])?;
        return Err(io::Error::other("injected write failure"));
    }
    f.write_all(bytes)
}

/// Remove ONE link `name` of `dir` (never its target), relative to the directory's handle: on
/// Windows a link carrying the directory attribute (a junction, a directory symlink) with
/// `remove_dir`, any other with `remove_file` (each falling back to the other); elsewhere
/// `remove_file`. Neither follows the link.
pub(crate) fn remove_link(dir: &Dir, name: &OsStr, dir_attr: bool) -> io::Result<()> {
    #[cfg(windows)]
    {
        if dir_attr {
            return dir.remove_dir(name).or_else(|_| dir.remove_file(name));
        }
        dir.remove_file(name).or_else(|_| dir.remove_dir(name))
    }
    #[cfg(not(windows))]
    {
        let _ = dir_attr;
        dir.remove_file(name)
    }
}

/// Remove the regular (or other non-directory, non-link) entry `name` of `dir` through its handle.
/// On Windows a read-only file is made writable first - through a handle opened on it without
/// following it - and the removal tried again.
fn remove_file_in(dir: &Dir, name: &OsStr) -> io::Result<()> {
    #[cfg(windows)]
    {
        match dir.remove_file(name) {
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                clear_readonly_and_remove(dir, name)
            }
            other => other,
        }
    }
    #[cfg(not(windows))]
    {
        dir.remove_file(name)
    }
}

/// (Windows) Clear the read-only attribute of the regular file `name` of `dir` through a handle
/// opened on it without following it, then remove it relative to the directory's handle.
#[cfg(windows)]
fn clear_readonly_and_remove(dir: &Dir, name: &OsStr) -> io::Result<()> {
    use cap_std::fs::OpenOptionsExt;
    const FILE_READ_ATTRIBUTES: u32 = 0x80;
    const FILE_WRITE_ATTRIBUTES: u32 = 0x100;
    let mut o = OpenOptions::new();
    o.access_mode(FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES)
        .follow(FollowSymlinks::No);
    let f = dir.open_with(name, &o)?;
    let m = f.metadata()?;
    if kind_of(&m) != Kind::File {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the entry is not a regular file",
        ));
    }
    let mut perm = m.permissions();
    perm.set_readonly(false);
    f.set_permissions(perm)?;
    drop(f);
    dir.remove_file(name)
}

/// One link a walk met: its directory's handle, the directory's root-relative path, its name, and
/// whether it carries the directory attribute ([`Kind::Link`]).
pub(crate) struct LinkAt<'a> {
    pub(crate) dir: &'a Dir,
    pub(crate) dir_rel: &'a Path,
    pub(crate) name: &'a OsStr,
    pub(crate) dir_attr: bool,
}

/// Walk every directory below `root` through handles and hand every LINK to `on_link` - never
/// following one, never descending into one. Each directory is listed through its own handle and
/// each subdirectory opened relative to its parent's handle without following it
/// ([`open_child`]); a subdirectory that turns out to be a link by then (replaced after it was
/// listed) is handed to `on_link` as a link. Every error (a directory that cannot be listed or
/// opened, an entry that cannot be read, an `on_link` error) is collected; with `fail_fast` the
/// walk stops at the first. Returns the errors (empty: a clean walk). `root` itself is not handed
/// over.
pub(crate) fn walk_links(
    root: &Root,
    fail_fast: bool,
    on_link: &mut dyn FnMut(&LinkAt<'_>) -> Result<(), String>,
) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();
    let first = match root.dir().try_clone() {
        Ok(d) => d,
        Err(e) => {
            errors.push(format!("{} cannot be opened ({e})", root.path().display()));
            return errors;
        }
    };
    let mut stack: Vec<(Dir, PathBuf)> = vec![(first, PathBuf::new())];
    while let Some((dir, rel)) = stack.pop() {
        let shown = root.path().join(&rel);
        let names = match list(&dir) {
            Ok(n) => n,
            Err(e) => {
                errors.push(format!("{} cannot be listed ({e})", shown.display()));
                if fail_fast {
                    return errors;
                }
                continue;
            }
        };
        for name in names {
            let at = shown.join(&name);
            let kind = match entry(&dir, &name) {
                Ok(Some(k)) => k,
                Ok(None) => continue,
                Err(e) => {
                    errors.push(format!("{} cannot be read ({e})", at.display()));
                    if fail_fast {
                        return errors;
                    }
                    continue;
                }
            };
            let link = match kind {
                Kind::Link { dir: d, .. } => Some(d),
                Kind::Dir => {
                    race_point("walk-descend");
                    match open_child(&dir, &name, false) {
                        Ok(sub) => {
                            stack.push((sub, rel.join(&name)));
                            None
                        }
                        // replaced by a link after it was listed: a link, never descended
                        Err(Step::Link { .. }) => Some(match entry(&dir, &name) {
                            Ok(Some(Kind::Link { dir: d, .. })) => d,
                            _ => true,
                        }),
                        Err(Step::Missing) => None,
                        Err(why) => {
                            errors.push(format!("{} {why}", at.display()));
                            if fail_fast {
                                return errors;
                            }
                            None
                        }
                    }
                }
                _ => None,
            };
            if let Some(dir_attr) = link {
                let l = LinkAt {
                    dir: &dir,
                    dir_rel: &rel,
                    name: &name,
                    dir_attr,
                };
                if let Err(why) = on_link(&l) {
                    errors.push(why);
                    if fail_fast {
                        return errors;
                    }
                }
            }
        }
    }
    errors
}

/// The unlink function of a removal: [`remove_link`] in production; a test injects a failure.
pub(crate) type UnlinkFn = fn(&Dir, &OsStr, bool) -> io::Result<()>;

/// (0.2.5 R7b, F43-6) At most this many entries a removal could not remove are named in its error.
pub(crate) const MAX_NAMED_FAILURES: usize = 10;

/// What a removal could not remove: the first [`MAX_NAMED_FAILURES`] root-relative paths with why,
/// and how many in all.
#[derive(Debug, Default)]
struct Failures {
    named: Vec<String>,
    total: usize,
}

impl Failures {
    fn add(&mut self, rel: &Path, why: String) {
        self.total += 1;
        if self.named.len() < MAX_NAMED_FAILURES {
            let shown: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect();
            let shown = if shown.is_empty() {
                ".".to_string()
            } else {
                shown.join("/")
            };
            self.named.push(format!("{shown}: {why}"));
        }
    }
}

/// Remove EVERYTHING below `root` through handles: a link is unlinked with `unlink` (never
/// followed), a file removed relative to its directory's handle (on Windows a read-only file has
/// its attribute cleared once and is tried again), a subdirectory opened relative to its parent's
/// handle without following it, emptied the same way and then removed relative to the parent's
/// handle (a directory that became a link meanwhile is unlinked instead). `root` itself is not
/// removed ([`remove_root`]).
///
/// (0.2.5 R7b, F43-6) An entry that cannot be removed (a locked file, a denied delete) does not stop
/// it: the walk goes on and removes everything it can; a directory that keeps such an entry is left
/// (it cannot be empty). `Err` then names up to [`MAX_NAMED_FAILURES`] of the entries left, by their
/// root-relative paths, and how many there are. The root-level entry `last` (a worktree's `.git`
/// file) is removed only once everything else is gone - a half-removed worktree stays one that git
/// and `c3 doctor` recognise, and `c3 doctor --fix` finishes it when the obstacle is gone.
pub(crate) fn remove_contents(
    root: &Root,
    unlink: UnlinkFn,
    last: Option<&OsStr>,
) -> Result<(), String> {
    fn remove_entry(dir: &Dir, rel: &Path, name: &OsStr, unlink: UnlinkFn, fails: &mut Failures) {
        let at = rel.join(name);
        let kind = match entry(dir, name) {
            Ok(Some(k)) => k,
            Ok(None) => return,
            Err(e) => {
                fails.add(&at, format!("cannot be read ({e})"));
                return;
            }
        };
        let link = match kind {
            Kind::Link { dir: d, .. } => Some(d),
            Kind::Dir => {
                race_point("walk-descend");
                match open_child(dir, name, false) {
                    Ok(sub) => {
                        let before = fails.total;
                        empty(&sub, &at, unlink, None, fails);
                        drop(sub);
                        if fails.total == before {
                            if let Err(e) = dir.remove_dir(name) {
                                fails.add(&at, format!("the directory could not be removed ({e})"));
                            }
                        }
                        None
                    }
                    Err(Step::Link { .. }) => Some(match entry(dir, name) {
                        Ok(Some(Kind::Link { dir: d, .. })) => d,
                        _ => true,
                    }),
                    Err(Step::Missing) => None,
                    Err(why) => {
                        fails.add(&at, why.to_string());
                        None
                    }
                }
            }
            _ => {
                if let Err(e) = remove_file_in(dir, name) {
                    fails.add(&at, format!("the file could not be removed ({e})"));
                }
                None
            }
        };
        if let Some(dir_attr) = link {
            match unlink(dir, name, dir_attr) {
                Err(e) => fails.add(&at, format!("the link could not be removed ({e})")),
                Ok(()) if entry(dir, name).ok().flatten().is_some() => {
                    fails.add(&at, "the link is still there".to_string())
                }
                Ok(()) => {}
            }
        }
    }
    fn empty(dir: &Dir, rel: &Path, unlink: UnlinkFn, last: Option<&OsStr>, fails: &mut Failures) {
        let names = match list(dir) {
            Ok(n) => n,
            Err(e) => {
                fails.add(rel, format!("cannot be listed ({e})"));
                return;
            }
        };
        let mut deferred: Option<OsString> = None;
        for name in names {
            if last.is_some_and(|l| l == name.as_os_str()) {
                deferred = Some(name);
                continue;
            }
            remove_entry(dir, rel, &name, unlink, fails);
        }
        if let Some(name) = deferred {
            if fails.total == 0 {
                remove_entry(dir, rel, &name, unlink, fails);
            }
        }
    }
    let mut fails = Failures::default();
    empty(root.dir(), Path::new(""), unlink, last, &mut fails);
    if fails.total == 0 {
        return Ok(());
    }
    let more = fails.total - fails.named.len();
    Err(format!(
        "{} entr{} below {} could not be removed and {} still there: {}{}",
        fails.total,
        if fails.total == 1 { "y" } else { "ies" },
        root.path().display(),
        if fails.total == 1 { "is" } else { "are" },
        fails.named.join("; "),
        if more > 0 {
            format!("; and {more} more")
        } else {
            String::new()
        }
    ))
}

/// Remove the (emptied, [`remove_contents`]) root directory itself: its parent is opened, the
/// entry is opened again from the parent's handle WITHOUT following it and must be the SAME
/// directory as `root` (its identity), then `root`'s handle is released and the entry removed
/// relative to the parent's handle - an empty directory or, if it was swapped in that instant, a
/// link (never followed); a non-empty directory is never removed. A root that is not there any
/// more is `Ok`.
pub(crate) fn remove_root(root: Root) -> Result<(), String> {
    let path = root.path().to_path_buf();
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(format!("{} has no parent directory", path.display()));
    };
    let parent_dir = Dir::open_ambient_dir(parent, ambient_authority()).map_err(|e| {
        format!(
            "the directory above {} cannot be opened ({e})",
            path.display()
        )
    })?;
    let again = match open_child(&parent_dir, name, false) {
        Ok(d) => d,
        Err(Step::Missing) => return Ok(()),
        Err(why) => return Err(format!("{} {why} now; it was not removed", path.display())),
    };
    let same = again
        .dir_metadata()
        .map(|m| identity(&m) == root.id())
        .unwrap_or(false);
    if !same {
        return Err(format!(
            "{} is no longer the directory that was emptied; it was not removed",
            path.display()
        ));
    }
    drop(again);
    drop(root);
    race_point("remove-root");
    match parent_dir.remove_dir(name) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("the directory could not be removed ({e})")),
    }
}

/// (0.2.5 D4) A race point: under [`RACE_HOOK`], swap the named directory for a link here (a test
/// proves that the operation then refuses, or completes strictly inside the directory it had
/// opened, never outside). Does nothing without the hook.
pub(crate) fn race_point(point: &str) {
    let Some(spec) = c3_core::test_hooks::hook(RACE_HOOK) else {
        return;
    };
    for line in spec.lines() {
        let mut parts = line.splitn(3, '|');
        let (Some(p), Some(dir), Some(target)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        if p != point {
            continue;
        }
        match dir.strip_prefix("write:") {
            Some(file) => {
                let _ = std::fs::write(file, target);
            }
            None => swap_for_link(Path::new(dir), Path::new(target)),
        }
    }
}

/// Rename `dir` aside (`<name>.c3-race-orig`) and make a link to `target` in its place: a junction
/// on Windows (no privilege needed), a symlink elsewhere. Nothing happens when the rename is refused
/// (on Windows: C3 holds the directory's handle).
fn swap_for_link(dir: &Path, target: &Path) {
    let Some(name) = dir.file_name() else {
        return;
    };
    let mut aside_name = name.to_os_string();
    aside_name.push(".c3-race-orig");
    let aside = dir.with_file_name(aside_name);
    if std::fs::rename(dir, &aside).is_err() {
        return;
    }
    #[cfg(windows)]
    {
        let win = |p: &Path| p.to_string_lossy().replace('/', "\\");
        let _ = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(win(dir))
            .arg(win(target))
            .output();
    }
    #[cfg(not(windows))]
    {
        let _ = std::os::unix::fs::symlink(target, dir);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A scratch layout: `<base>/root/inner/own.txt` and `<base>/outside/sentinel.txt` (+ `x/`).
    pub(crate) fn scratch(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!(
            "c3-contain-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let root = base.join("root");
        let outside = base.join("outside");
        std::fs::create_dir_all(root.join("inner")).unwrap();
        std::fs::create_dir_all(outside.join("x")).unwrap();
        std::fs::write(outside.join("sentinel.txt"), "OUTSIDE\n").unwrap();
        std::fs::write(root.join("inner").join("own.txt"), "INSIDE\n").unwrap();
        (base, root, outside)
    }

    /// A directory link at `link` to `target`: a junction on Windows (no privilege needed), a
    /// symlink elsewhere.
    pub(crate) fn junction_or_symlink(link: &Path, target: &Path) {
        #[cfg(windows)]
        {
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

    /// A directory SYMLINK at `link` to `target` (relative or absolute); `false` when the system
    /// refuses to make one (on Windows it needs a privilege or developer mode).
    fn try_dir_symlink(link: &Path, target: &Path) -> bool {
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(target, link).is_ok()
        }
        #[cfg(not(windows))]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
    }

    /// The names in `dir` (to prove nothing was created there).
    pub(crate) fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        v.sort();
        v
    }

    /// Every operation on a path whose component `link` is a link must refuse (or report the link)
    /// and never reach `outside`.
    fn assert_refused_through(root: &Root, link: &str, outside: &Path) {
        let before = names(outside);
        let l = Path::new(link);
        // the contained walk: the link component refuses, as a link
        for rel in [l.to_path_buf(), l.join("x"), l.join("sentinel.txt")] {
            match root.open_dir(&rel, false) {
                Err(StepError {
                    why: Step::Link { .. },
                    at,
                }) => assert_eq!(at, l, "{rel:?}"),
                other => panic!("open_dir({rel:?}) through `{link}`: {other:?}"),
            }
        }
        match root.open_dir(&l.join("newdir"), true) {
            Err(StepError {
                why: Step::Link { .. },
                ..
            }) => {}
            other => panic!("open_dir(create) through `{link}`: {other:?}"),
        }
        match root.read(&l.join("sentinel.txt")) {
            Err(StepError {
                why: Step::Link { .. },
                ..
            }) => {}
            other => panic!("read through `{link}`: {other:?}"),
        }
        match root.kind(l) {
            Ok(Some(Kind::Link { .. })) => {}
            other => panic!("kind(`{link}`): {other:?}"),
        }
        // cap-std itself: the no-follow open refuses the link ...
        assert!(root.dir().open_dir_nofollow(l).is_err(), "{link}");
        // ... and its own resolution never leaves the root through it, even asked to follow
        assert!(
            root.dir().create_dir(l.join("via-cap-std")).is_err() || {
                // a link whose target lies inside the root may be followed by cap-std itself
                // (never by C3's walk above); it then never reaches `outside`
                !outside.join("via-cap-std").exists()
            }
        );
        let mut o = OpenOptions::new();
        o.write(true).create_new(true);
        let _ = root.dir().open_with(l.join("via-cap-std.txt"), &o);
        assert_eq!(names(outside), before, "nothing was created outside");
        assert_eq!(
            std::fs::read_to_string(outside.join("sentinel.txt")).unwrap(),
            "OUTSIDE\n"
        );
    }

    // (0.2.5 D4) THE PROOF the wave stands on: cap-std, used component by component without
    // following, refuses a JUNCTION (Windows) and a DIRECTORY SYMLINK (every platform where one can
    // be made) as a path component - pointing outside the root, and pointing INSIDE it (C3 refuses
    // a link even where it would stay inside) - and a root that is itself a link. On Windows, while
    // a directory handle is held, neither that directory nor an ancestor can be renamed - what keeps
    // the path-built operations (rename, remove, create a directory) on the opened directory.
    #[test]
    fn cap_std_refuses_a_link_as_a_path_component() {
        let (base, root_p, outside) = scratch("proof");
        // a junction (Windows) / symlink (elsewhere) to the outside directory, and one to `inner`
        junction_or_symlink(&root_p.join("j"), &outside);
        junction_or_symlink(&root_p.join("jin"), &root_p.join("inner"));
        let root = Root::open(&root_p).unwrap();
        assert_refused_through(&root, "j", &outside);
        assert_refused_through(&root, "jin", &outside);
        // directory SYMLINKS (absolute out, relative out, relative in), where the system allows
        let made: Vec<&str> = [
            ("sabs", outside.clone()),
            ("srel", PathBuf::from("..").join("outside")),
            ("sin", PathBuf::from("inner")),
        ]
        .iter()
        .filter(|(n, t)| try_dir_symlink(&root_p.join(n), t))
        .map(|(n, _)| *n)
        .collect();
        if made.is_empty() {
            eprintln!(
                "skipped the directory-symlink half of cap_std_refuses_a_link_as_a_path_component: this system refused to create a directory symlink (it needs a privilege or developer mode on Windows); the junction half ran"
            );
        }
        for n in &made {
            assert_refused_through(&root, n, &outside);
        }
        // the link entries are reported as links, read through the root's handle
        let kinds: Vec<(String, Kind)> = list(root.dir())
            .unwrap()
            .into_iter()
            .filter_map(|n| {
                let k = entry(root.dir(), &n).unwrap()?;
                Some((n.to_string_lossy().to_string(), k))
            })
            .collect();
        for (n, k) in &kinds {
            if n != "inner" {
                assert!(matches!(k, Kind::Link { .. }), "{n}: {k:?}");
            }
        }
        // a root that is itself a link is refused by the no-follow open
        let lroot = base.join("lroot");
        junction_or_symlink(&lroot, &outside);
        assert!(matches!(Root::open(&lroot), Err(RootError::Link)));
        // the walk reports every link and descends into none
        let mut seen: Vec<String> = Vec::new();
        let errs = walk_links(&root, false, &mut |l| {
            seen.push(l.dir_rel.join(l.name).to_string_lossy().replace('\\', "/"));
            Ok(())
        });
        assert!(errs.is_empty(), "{errs:?}");
        seen.sort();
        let mut want: Vec<String> = vec!["j".into(), "jin".into()];
        want.extend(made.iter().map(|s| s.to_string()));
        want.sort();
        assert_eq!(seen, want);
        // Windows: a held directory pins itself and every ancestor
        #[cfg(windows)]
        {
            let inner = root.open_dir(Path::new("inner"), false).unwrap();
            drop(root);
            assert!(
                std::fs::rename(root_p.join("inner"), root_p.join("inner2")).is_err(),
                "a held directory cannot be renamed"
            );
            assert!(
                std::fs::rename(&root_p, base.join("root2")).is_err(),
                "an ancestor of a held directory cannot be renamed"
            );
            drop(inner);
            std::fs::rename(root_p.join("inner"), root_p.join("inner2")).unwrap();
        }
        #[cfg(not(windows))]
        drop(root);
        // clean up the links themselves, never their targets
        for n in ["j", "jin", "sabs", "srel", "sin"] {
            let p = root_p.join(n);
            if let Ok(m) = std::fs::symlink_metadata(&p) {
                if crate::engines::worktree::is_link(&m) {
                    let _ = std::fs::remove_dir(&p).or_else(|_| std::fs::remove_file(&p));
                }
            }
        }
        let _ = std::fs::remove_dir(&lroot).or_else(|_| std::fs::remove_file(&lroot));
        assert_eq!(
            std::fs::read_to_string(outside.join("sentinel.txt")).unwrap(),
            "OUTSIDE\n"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    // (0.2.5 D4) The removal through handles takes a read-only file too (on Windows its attribute
    // is cleared through a handle first), then the emptied root itself.
    #[test]
    fn a_tree_with_a_read_only_file_is_removed_through_its_handles() {
        let (base, root_p, outside) = scratch("readonly");
        let ro = root_p.join("inner").join("ro.txt");
        std::fs::write(&ro, "x\n").unwrap();
        let mut perm = std::fs::metadata(&ro).unwrap().permissions();
        perm.set_readonly(true);
        std::fs::set_permissions(&ro, perm).unwrap();
        junction_or_symlink(&root_p.join("out"), &outside);
        let root = Root::open(&root_p).unwrap();
        remove_contents(&root, remove_link, None).unwrap();
        assert!(names(&root_p).is_empty());
        remove_root(root).unwrap();
        assert!(std::fs::symlink_metadata(&root_p).is_err());
        assert_eq!(
            names(&outside),
            vec!["sentinel.txt".to_string(), "x".to_string()]
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    // (0.2.5 D4, F16-5) THE ROOT-REMOVAL RACE: the emptied root is swapped for a link to an outside
    // directory after it was checked to be the same directory and released (`remove-root`): the
    // final removal is relative to the parent's handle and never follows the link - on Windows it
    // removes the link itself, elsewhere it refuses (a link is no directory to remove); the outside
    // directory keeps everything. A root that is no longer the directory that was emptied is not
    // removed at all.
    #[test]
    fn the_root_swapped_for_a_link_before_its_removal_is_never_followed() {
        let (base, root_p, outside) = scratch("rootrace");
        let root = Root::open(&root_p).unwrap();
        remove_contents(&root, remove_link, None).unwrap();
        let hook = c3_core::test_hooks::set_thread_hook(
            RACE_HOOK,
            &format!("remove-root|{}|{}", root_p.display(), outside.display()),
        );
        let r = remove_root(root);
        drop(hook);
        let aside = base.join("root.c3-race-orig");
        if aside.exists() {
            match &r {
                Ok(()) => assert!(
                    std::fs::symlink_metadata(&root_p).is_err(),
                    "the link itself was removed"
                ),
                Err(why) => assert!(why.contains("could not be removed"), "{why}"),
            }
        } else {
            assert_eq!(r, Ok(()));
        }
        assert_eq!(
            names(&outside),
            vec!["sentinel.txt".to_string(), "x".to_string()]
        );
        // a root replaced by ANOTHER real directory is not the one that was emptied
        let other_p = base.join("other");
        std::fs::create_dir_all(&other_p).unwrap();
        let other = Root::open(&other_p).unwrap();
        drop(other);
        let other = Root::open(&other_p).unwrap();
        std::fs::create_dir_all(base.join("swap")).unwrap();
        let held_id = other.id();
        // emulate: same path, other directory (rename the original away, put a new one in place)
        let moved = base.join("other-moved");
        #[cfg(windows)]
        let renamed = {
            // a held directory cannot be renamed on Windows: the identity check is shown on the
            // directory itself instead
            let _ = &moved;
            false
        };
        #[cfg(not(windows))]
        let renamed = std::fs::rename(&other_p, &moved).is_ok();
        if renamed {
            std::fs::create_dir_all(&other_p).unwrap();
            let why = remove_root(other).unwrap_err();
            assert!(why.contains("no longer the directory"), "{why}");
            assert!(other_p.exists() && moved.exists());
        } else {
            assert_eq!(other.id(), held_id);
            drop(other);
        }
        let _ = std::fs::remove_dir(&root_p).or_else(|_| std::fs::remove_file(&root_p));
        let _ = std::fs::remove_dir_all(&base);
    }

    // (0.2.5 D4) publish: an exclusive temp, a rename relative to the handle; a refused check or an
    // injected write failure leaves neither a temp nor a destination; a cleanup that fails would be
    // reported (the message names the temp).
    #[test]
    fn publish_writes_by_rename_and_leaves_nothing_on_failure() {
        let (base, root_p, _outside) = scratch("publish");
        let root = Root::open(&root_p).unwrap();
        let (dir, leaf) = root.open_parent(Path::new("a/b/new.txt"), true).unwrap();
        let ok = |_: Stage| -> Result<(), String> { Ok(()) };
        let p = Publish {
            dir: &dir,
            leaf: &leaf,
            tmp: OsStr::new(".t1.tmp"),
            bytes: b"hello\n",
            permissions: None,
            check: &ok,
            race: "test",
        };
        publish(&p).unwrap();
        assert_eq!(
            std::fs::read_to_string(root_p.join("a/b/new.txt")).unwrap(),
            "hello\n"
        );
        // a check that refuses before the rename: the temp is gone, the file keeps its bytes
        let late = |s: Stage| -> Result<(), String> {
            if s == Stage::BeforeRename {
                Err("refused late".to_string())
            } else {
                Ok(())
            }
        };
        let p2 = Publish {
            bytes: b"other\n",
            check: &late,
            tmp: OsStr::new(".t2.tmp"),
            ..p
        };
        let f = publish(&p2).unwrap_err();
        assert!(matches!(f.error, PublishError::Check(_)) && f.cleanup.is_none());
        // an injected write failure (half the bytes written): no temp, no change
        let _hook = c3_core::test_hooks::set_thread_hook(WRITE_FAIL_HOOK, "1");
        let p3 = Publish {
            tmp: OsStr::new(".t3.tmp"),
            check: &ok,
            ..p2
        };
        let f = publish(&p3).unwrap_err();
        assert!(matches!(f.error, PublishError::Write(_)), "{f:?}");
        assert_eq!(names(&root_p.join("a/b")), vec!["new.txt".to_string()]);
        assert_eq!(
            std::fs::read_to_string(root_p.join("a/b/new.txt")).unwrap(),
            "hello\n"
        );
        assert_eq!(f.with_cleanup("x".into()), "x");
        drop(dir);
        drop(root);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Whether this build has an atomic exchange ([`exchange`]): Windows, Linux, Android, Apple.
    const HAS_EXCHANGE: bool = cfg!(any(
        windows,
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple"
    ));

    /// A [`publish_exchange`] of `bytes` onto `<root>/inner/own.txt` whose check (at both stages)
    /// requires the file to read as `basis` - the comparison of the roster writer and the repair.
    fn exchange_own(root: &Root, tmp: &str, bytes: &[u8], basis: &[u8]) -> Exchanged {
        let (dir, leaf) = root.open_parent(Path::new("inner/own.txt"), false).unwrap();
        let check = |_: Stage| -> Result<(), String> {
            match read_file_in(&dir, &leaf) {
                Ok(b) if b == basis => Ok(()),
                _ => Err("changed".to_string()),
            }
        };
        publish_exchange(
            &Publish {
                dir: &dir,
                leaf: &leaf,
                tmp: OsStr::new(tmp),
                bytes,
                permissions: None,
                check: &check,
                race: "x",
            },
            basis,
        )
        .unwrap()
    }

    // (F57-10, F02-2) An editor's save that lands AFTER the last comparison and before the
    // publication (the race hook writes it at `x-publish`) is displaced, not lost: the file holds the
    // new bytes, the edit is kept byte for byte as `own.txt.displaced-<UTC time>` and returned; on
    // this platform the publication was the atomic exchange. Without an edit nothing is left behind.
    #[test]
    fn publish_exchange_keeps_an_edit_saved_before_the_exchange_and_nothing_without_one() {
        let (base, root_p, _outside) = scratch("exchange");
        let root = Root::open(&root_p).unwrap();
        let file = root_p.join("inner").join("own.txt");
        let hook = c3_core::test_hooks::set_thread_hook(
            RACE_HOOK,
            &format!(
                "x-publish|write:{}|EDITED BY HAND, unrelated line",
                file.display()
            ),
        );
        let done = exchange_own(&root, ".own.t1.tmp", b"NEW\n", b"INSIDE\n");
        drop(hook);
        assert_eq!(std::fs::read(&file).unwrap(), b"NEW\n");
        if HAS_EXCHANGE {
            assert_eq!(done.how, Publication::Exchange);
        }
        let kept = done.kept.clone().expect("the displaced edit is kept");
        assert_eq!(kept.unproven, None, "it differs from the basis");
        let kept_name = kept.name.to_string_lossy().to_string();
        assert!(kept_name.starts_with("own.txt.displaced-"), "{kept_name}");
        assert_eq!(
            std::fs::read(root_p.join("inner").join(&kept_name)).unwrap(),
            b"EDITED BY HAND, unrelated line"
        );
        assert_eq!(
            names(&root_p.join("inner")),
            vec!["own.txt".to_string(), kept_name.clone()]
        );
        let notice = done
            .notice("own.txt", |n| n.to_string_lossy().to_string())
            .unwrap();
        assert!(
            notice.contains("a hand edit of own.txt landed during C3's write")
                && notice.contains(&kept_name),
            "{notice}"
        );
        // no edit: the displaced version IS the basis - removed, nothing left, nothing to say
        let done = exchange_own(&root, ".own.t2.tmp", b"NEWER\n", b"NEW\n");
        assert_eq!(done.kept, None);
        assert_eq!(done.notice("own.txt", |_| String::new()), None);
        assert_eq!(std::fs::read(&file).unwrap(), b"NEWER\n");
        assert_eq!(
            names(&root_p.join("inner")),
            vec!["own.txt".to_string(), kept_name]
        );
        drop(root);
        let _ = std::fs::remove_dir_all(&base);
    }

    // (F57-10, F02-2) Without an atomic exchange (forced by its test hook) the publication is the
    // compare-then-rename it was before and SAYS so: the new bytes are in place, nothing is left
    // behind, and an edit landing after the last comparison is not kept - the limit that remains on
    // such a system, named in the notice. A file that is not there is published without replacing
    // anything.
    #[test]
    fn without_an_exchange_publication_falls_back_to_the_rename_and_says_so() {
        let (base, root_p, _outside) = scratch("no-exchange");
        let root = Root::open(&root_p).unwrap();
        let file = root_p.join("inner").join("own.txt");
        let _off = c3_core::test_hooks::set_thread_hook(NO_EXCHANGE_HOOK, "1");
        let done = exchange_own(&root, ".own.t1.tmp", b"NEW\n", b"INSIDE\n");
        assert!(
            matches!(&done.how, Publication::Rename(why) if why.contains("test hook")),
            "{done:?}"
        );
        assert_eq!(done.kept, None);
        assert_eq!(std::fs::read(&file).unwrap(), b"NEW\n");
        assert_eq!(names(&root_p.join("inner")), vec!["own.txt".to_string()]);
        let notice = done.notice("own.txt", |_| String::new()).unwrap();
        assert!(
            notice.contains("replaced by rename, not by an atomic exchange"),
            "{notice}"
        );
        // an edit at the same moment is lost here (what the exchange closes elsewhere)
        let edit = c3_core::test_hooks::set_thread_hook(
            RACE_HOOK,
            &format!("x-publish|write:{}|EDITED", file.display()),
        );
        let done = exchange_own(&root, ".own.t2.tmp", b"NEWER\n", b"NEW\n");
        drop(edit);
        assert_eq!(done.kept, None);
        assert_eq!(std::fs::read(&file).unwrap(), b"NEWER\n");
        assert_eq!(names(&root_p.join("inner")), vec!["own.txt".to_string()]);
        drop(_off);
        // a first write: nothing to displace, nothing replaced
        let dir = root.open_dir(Path::new("inner"), false).unwrap();
        let any = |_: Stage| -> Result<(), String> { Ok(()) };
        let done = publish_exchange(
            &Publish {
                dir: &dir,
                leaf: OsStr::new("first.txt"),
                tmp: OsStr::new(".first.tmp"),
                bytes: b"FIRST\n",
                permissions: None,
                check: &any,
                race: "x",
            },
            b"",
        )
        .unwrap();
        if HAS_EXCHANGE {
            assert_eq!(done.how, Publication::Created);
        }
        assert_eq!(done.kept, None);
        assert_eq!(
            std::fs::read(root_p.join("inner").join("first.txt")).unwrap(),
            b"FIRST\n"
        );
        assert_eq!(
            names(&root_p.join("inner")),
            vec!["first.txt".to_string(), "own.txt".to_string()]
        );
        drop(dir);
        drop(root);
        let _ = std::fs::remove_dir_all(&base);
    }

    // (F57-10, F02-2) The Unix twin: the exchange is `renameat2(RENAME_EXCHANGE)` /
    // `renamex_np(RENAME_SWAP)` relative to the directory's descriptor - an edit saved before it is
    // kept byte for byte, the new file carries the permission bits the caller gave it, and the
    // directory renamed away meanwhile (the descriptor still names it) receives the publication.
    #[cfg(unix)]
    #[test]
    fn publish_exchange_on_unix_swaps_relative_to_the_directory_descriptor() {
        use std::os::unix::fs::PermissionsExt;
        let (base, root_p, _outside) = scratch("exchange-unix");
        let root = Root::open(&root_p).unwrap();
        let (dir, leaf) = root.open_parent(Path::new("inner/own.txt"), false).unwrap();
        let moved = root_p.join("inner-moved");
        let file = moved.join("own.txt");
        // the directory is renamed before the temp file is made; the edit lands before the swap
        let hook = c3_core::test_hooks::set_thread_hook(
            RACE_HOOK,
            &format!("x-publish|write:{}|EDITED BY HAND", file.display()),
        );
        std::fs::rename(root_p.join("inner"), &moved).unwrap();
        let check = |_: Stage| -> Result<(), String> { Ok(()) };
        let done = publish_exchange(
            &Publish {
                dir: &dir,
                leaf: &leaf,
                tmp: OsStr::new(".own.tmp"),
                bytes: b"NEW\n",
                permissions: Some(cap_std::fs::Permissions::from_std(
                    std::fs::Permissions::from_mode(0o640),
                )),
                check: &check,
                race: "x",
            },
            b"INSIDE\n",
        )
        .unwrap();
        drop(hook);
        if HAS_EXCHANGE {
            assert_eq!(done.how, Publication::Exchange);
        }
        assert_eq!(std::fs::read(&file).unwrap(), b"NEW\n");
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
        let kept = done.kept.expect("kept").name.to_string_lossy().to_string();
        assert!(kept.starts_with("own.txt.displaced-"), "{kept}");
        assert_eq!(std::fs::read(moved.join(&kept)).unwrap(), b"EDITED BY HAND");
        assert_eq!(names(&moved), vec!["own.txt".to_string(), kept]);
        drop(dir);
        drop(root);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// An injected unlink failure (a link held open, a denied delete).
    fn failing_unlink(_dir: &Dir, _name: &OsStr, _dir_attr: bool) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "injected unlink failure",
        ))
    }

    // (0.2.5 R7b, F43-6) A removal that meets entries it cannot remove goes on: everything else is
    // removed, a directory that keeps such an entry stays, the error names the first ten left by
    // their root-relative paths and counts the rest, and the root-level `last` entry (a worktree's
    // `.git`) is kept while anything is left. Once the obstacle is gone the same removal finishes,
    // `last` included.
    #[test]
    fn a_failed_removal_removes_what_it_can_and_names_what_is_left() {
        let (base, root_p, outside) = scratch("partial");
        std::fs::write(root_p.join(".git"), "gitdir: elsewhere\n").unwrap();
        std::fs::write(root_p.join("top.txt"), "x\n").unwrap();
        std::fs::create_dir_all(root_p.join("clean").join("deep")).unwrap();
        std::fs::write(root_p.join("clean").join("deep").join("f.txt"), "x\n").unwrap();
        std::fs::create_dir_all(root_p.join("held")).unwrap();
        std::fs::write(root_p.join("held").join("own.txt"), "x\n").unwrap();
        for i in 0..12 {
            junction_or_symlink(&root_p.join("held").join(format!("l{i:02}")), &outside);
        }
        let root = Root::open(&root_p).unwrap();
        let why = remove_contents(&root, failing_unlink, Some(OsStr::new(".git"))).unwrap_err();
        assert!(
            why.starts_with("12 entries below ") && why.contains("could not be removed"),
            "{why}"
        );
        assert!(
            why.contains("held/l00: the link could not be removed"),
            "{why}"
        );
        assert!(
            why.contains("held/l09:") && !why.contains("held/l10:"),
            "{why}"
        );
        assert!(why.ends_with("; and 2 more"), "{why}");
        // what could be removed is gone; the held directory, its links and `.git` stay
        assert_eq!(names(&root_p), vec![".git".to_string(), "held".to_string()]);
        assert_eq!(names(&root_p.join("held")).len(), 12);
        assert_eq!(
            names(&outside),
            vec!["sentinel.txt".to_string(), "x".to_string()]
        );
        // the obstacle gone: the same removal finishes, `.git` last
        remove_contents(&root, remove_link, Some(OsStr::new(".git"))).unwrap();
        assert!(names(&root_p).is_empty());
        remove_root(root).unwrap();
        assert!(std::fs::symlink_metadata(&root_p).is_err());
        assert_eq!(
            names(&outside),
            vec!["sentinel.txt".to_string(), "x".to_string()]
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    // (0.2.5 R7b, F44-3) A FIFO swapped in after the kind check is opened without blocking and
    // refused on its handle - never read, never waited on. (Unix only: Windows keeps named pipes
    // out of the file system.)
    #[cfg(unix)]
    #[test]
    fn a_fifo_swapped_in_after_the_kind_check_is_refused_without_blocking() {
        let (base, root_p, _outside) = scratch("fifo");
        let fifo = root_p.join("inner").join("pipe");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !made {
            eprintln!("skipped: mkfifo is not available");
            let _ = std::fs::remove_dir_all(&base);
            return;
        }
        let root = Root::open(&root_p).unwrap();
        let dir = root.open_dir(Path::new("inner"), false).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            // the open right after a kind check that saw a regular file
            let r = open_checked(&dir, OsStr::new("pipe")).map(|_| ());
            let _ = tx.send(format!("{r:?}"));
        });
        let got = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the open of a FIFO blocked");
        assert!(got.contains("NotFile"), "{got}");
        // the full path refuses it before it is opened
        assert!(matches!(
            root.read(Path::new("inner/pipe")),
            Err(StepError {
                why: Step::NotFile,
                ..
            })
        ));
        let _ = std::fs::remove_dir_all(&base);
    }
}
