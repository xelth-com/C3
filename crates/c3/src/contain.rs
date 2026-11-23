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

/// The options of a read-only open that does not follow the final component.
fn nofollow_read() -> OpenOptions {
    let mut o = OpenOptions::new();
    o.read(true).follow(FollowSymlinks::No);
    o
}

/// Open the file `name` of `dir` for reading WITHOUT following it; the handle is checked to be a
/// regular file (not a link, not a reparse point, not a directory or device). A name that is not a
/// regular file when read without following it is refused BEFORE it is opened (so a FIFO is never
/// opened); the handle check catches an entry swapped in between.
pub(crate) fn open_file_in(dir: &Dir, name: &OsStr) -> Result<File, Step> {
    match entry(dir, name) {
        Ok(None) => return Err(Step::Missing),
        Ok(Some(Kind::Link { symlink, .. })) => return Err(Step::Link { symlink }),
        Ok(Some(Kind::File)) => {}
        Ok(Some(_)) => return Err(Step::NotFile),
        Err(e) => return Err(Step::Io(e)),
    }
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
    let fail = |error: PublishError, cleanup: Option<io::Error>| PublishFailure {
        error,
        cleanup,
        tmp: p.tmp.to_os_string(),
    };
    (p.check)(Stage::BeforeCreate).map_err(|why| fail(PublishError::Check(why), None))?;
    race_point(&format!("{}-create", p.race));
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    let mut f = p
        .dir
        .open_with(p.tmp, &o)
        .map_err(|e| fail(PublishError::Create(e), None))?;
    let remove_tmp = || match p.dir.remove_file(p.tmp) {
        Ok(()) => None,
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => Some(e),
    };
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
        return Err(fail(PublishError::Write(e), remove_tmp()));
    }
    if let Err(why) = (p.check)(Stage::BeforeRename) {
        return Err(fail(PublishError::Check(why), remove_tmp()));
    }
    race_point(&format!("{}-publish", p.race));
    p.dir
        .rename(p.tmp, p.dir, p.leaf)
        .map_err(|e| fail(PublishError::Rename(e), remove_tmp()))
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

/// Remove EVERYTHING below `root` through handles: a link is unlinked with `unlink` (never
/// followed), a file removed relative to its directory's handle, a subdirectory opened relative to
/// its parent's handle without following it, emptied the same way and then removed relative to the
/// parent's handle (a directory that became a link meanwhile is unlinked instead). The first error
/// stops it: `Err` names it and what is left stays. `root` itself is not removed ([`remove_root`]).
pub(crate) fn remove_contents(root: &Root, unlink: UnlinkFn) -> Result<(), String> {
    fn empty(dir: &Dir, shown: &Path, unlink: UnlinkFn) -> Result<(), String> {
        let names = list(dir).map_err(|e| format!("{} cannot be listed ({e})", shown.display()))?;
        for name in names {
            let at = shown.join(&name);
            let kind = match entry(dir, &name) {
                Ok(Some(k)) => k,
                Ok(None) => continue,
                Err(e) => return Err(format!("{} cannot be read ({e})", at.display())),
            };
            let link = match kind {
                Kind::Link { dir: d, .. } => Some(d),
                Kind::Dir => {
                    race_point("walk-descend");
                    match open_child(dir, &name, false) {
                        Ok(sub) => {
                            empty(&sub, &at, unlink)?;
                            drop(sub);
                            dir.remove_dir(&name).map_err(|e| {
                                format!("the directory {} could not be removed ({e})", at.display())
                            })?;
                            None
                        }
                        Err(Step::Link { .. }) => Some(match entry(dir, &name) {
                            Ok(Some(Kind::Link { dir: d, .. })) => d,
                            _ => true,
                        }),
                        Err(Step::Missing) => None,
                        Err(why) => return Err(format!("{} {why}", at.display())),
                    }
                }
                _ => {
                    remove_file_in(dir, &name).map_err(|e| {
                        format!("the file {} could not be removed ({e})", at.display())
                    })?;
                    None
                }
            };
            if let Some(dir_attr) = link {
                unlink(dir, &name, dir_attr)
                    .map_err(|e| format!("the link {} could not be removed ({e})", at.display()))?;
                if entry(dir, &name).ok().flatten().is_some() {
                    return Err(format!("the link {} is still there", at.display()));
                }
            }
        }
        Ok(())
    }
    empty(root.dir(), root.path(), unlink)
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
        if p == point {
            swap_for_link(Path::new(dir), Path::new(target));
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
        remove_contents(&root, remove_link).unwrap();
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
        remove_contents(&root, remove_link).unwrap();
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
}
