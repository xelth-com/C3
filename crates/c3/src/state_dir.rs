//! C3's per-project folder (0.2.5, wave S1): `<repo>/.c3/`, lowercase, at the repository root,
//! git-ignored as a whole. It holds the manifests (`ROADMAP.md`, `CONTEXT.md`, ... and
//! `profiles.json`), `last-report.md` (the report of the last `c3 finish` / `c3 fail`), `scouts/`,
//! the hand-off `snapshots/` and `lastsnapshot/` (written only on demand), and C3's own cache -
//! `anchor`, `snapshot_seq`, `index/`, `explains/` - one per repository.
//!
//! The folder was inherited as `.eck/` from another tool. This module is the ONE place that names
//! both folders ([`STATE_DIR`], [`LEGACY_STATE_DIR`]) and the ignore files ([`IGNORE_FILE`], with
//! [`LEGACY_IGNORE_FILE`] as the fallback), and it holds the operations on the folder:
//!
//! * [`ensure`] - called by every command that WRITES into the folder: it moves an existing `.eck/`
//!   once ([`migrate_legacy`]), finishes a move an S1 build left half done, moves a legacy cache
//!   `<collab root>/.c3/` entry by entry ([`migrate_collab_cache`]), creates `.c3/`, writes each
//!   missing stub manifest and adds `.c3/` to `.gitignore` when no line ignores it. Inside a C3 child
//!   (`C3_CHILD`) it creates and moves nothing: a folder that is not there is a refusal.
//! * [`read_path`] - what the read-only commands read: `.c3/`, else `.eck/` until a writing command
//!   moved it.
//! * [`collab_dir`] / [`collab_dir_moving`] - (point 11) a collab directory kept inside the folder
//!   (`.eck/collab`) follows the move; every collab path a command resolves goes through one of them.
//!   (S2, F36-4) Only [`collab_dir_moving`] may move the folder, and only a real consultation or work
//!   start calls it; [`collab_dir`], the default, never moves anything.
//! * [`prepare_dir`] / [`write_in`] - (S2, F36-1/F36-3) where a writer puts a file: always the
//!   folder as it is NOW (a path in `.eck/` that the move carried to `.c3/` is taken there), never
//!   through a link, and a legacy folder that is gone is never created again.
//! * [`begin_run`] - (S2, F36-6) a consultation or work run that starts with its records under the
//!   legacy folder creates them under the move lock, so it cannot begin between the live-run check
//!   and the rename.
//!
//! (S2) The move is taken under an exclusive lock every c3 process of the repository sees
//! ([`MoveLock`]: a file created with exclusive create in the repository's git directory, holding
//! the owner's pid and start time); the state is read again under the lock, the live-run check
//! ([`live_run_under`], fail closed) is repeated there, the old tool's leftovers are relocated
//! INSIDE `.eck/` first and the rename is the last step - so a `.c3/` never holds the old tool's
//! `anchor` at its top. A process that cannot take the lock, and any rename error, reads the state
//! again before it concludes anything. The move never runs through a link (a `.eck` that is a
//! symlink, a junction or another reparse point is left alone; a `.c3` that is one is refused by
//! every writing command), never over an existing path (`.c3` in any form stops it), never while a
//! run may be live under `.eck/`, and when it cannot be made now the command goes on with the
//! legacy folder for that run and says so. It is off with `C3_STATE_MIGRATE=off` or
//! `"state_migrate": false` in the C3 config (then `.eck/` stays and C3 creates and uses `.c3/` for
//! its own files; a collab directory placed under `.eck/` is used as given). The SessionStart hook
//! never moves anything ([`forbid_moves`]).

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use crate::setup::init::{ensure_gitignore_line, stub_content, STUB_MANIFESTS};
use crate::setup::repo_models::{is_link, link_words};

/// The folder's name, at the repository root.
pub const STATE_DIR: &str = ".c3";
/// The name the folder had before (another tool's), read as a fallback and moved once.
pub const LEGACY_STATE_DIR: &str = ".eck";
/// The project's "never put in AI context" list (gitignore syntax, one per directory).
pub const IGNORE_FILE: &str = ".c3ignore";
/// The list's old name, read in a directory that has no [`IGNORE_FILE`].
pub const LEGACY_IGNORE_FILE: &str = ".eckignore";
/// The `.gitignore` line [`ensure`] adds.
pub const GITIGNORE_LINE: &str = ".c3/";
/// The report of the last `c3 finish` / `c3 fail`, directly in the folder.
pub const LAST_REPORT: &str = "last-report.md";
/// Where the old tool's leftovers go inside the folder.
pub const LEGACY_SUBDIR: &str = "legacy";
/// The switch that keeps C3 from moving `.eck/` (`off`).
pub const MIGRATE_ENV: &str = "C3_STATE_MIGRATE";

/// The old tool's files at the top of its folder that C3 never reads: moved into
/// [`LEGACY_SUBDIR`] (`anchor` in particular must not be taken for C3's own anchor).
const LEGACY_LEFTOVERS: &[&str] = &["anchor", "update_seq", "telemetry_queue.json"];
/// (S2, F36-2) The old tool's files that only it writes: one of them at the top of `.c3/` marks a
/// move an S1 build left half done (its `anchor` beside them is the old tool's too).
const HALF_DONE_MARKERS: &[&str] = &["update_seq", "telemetry_queue.json"];
/// The old tool's report, relative to the folder.
const LEGACY_REPORT: [&str; 2] = ["lastsnapshot", "AnswerToSA.md"];
/// The directories of the folder where no task directory lives (the live-run walk does not look
/// into them, and a link there hides no run).
const NOT_RUN_PLACES: &[&str] = &[
    "snapshots",
    "lastsnapshot",
    "scouts",
    "index",
    "explains",
    "legacy",
];

/// How a refusal inside a C3 child reads.
pub const CHILD_REFUSAL: &str = "C3 does not create or move its .c3/ folder inside a C3 child (C3_CHILD is set) - run the command outside the reviewer or worker";

/// The test hook that makes the folder's rename fail (honoured only in test mode), so a failed
/// move can be tested without holding a file open.
pub const RENAME_FAIL_HOOK: &str = "CODEX_CONSULT_TEST_STATE_RENAME_FAIL";
/// (S2) The test hook that holds a mover inside the move (test mode only): with the lock taken and
/// the checks passed, it creates `<value>.held` and waits (at most 60 s) until `<value>` exists.
pub const MOVE_HOLD_HOOK: &str = "CODEX_CONSULT_TEST_STATE_MOVE_HOLD";
/// (S2) The test hook that ends the process between the leftover steps and the rename (test mode
/// only; exit code 86), as a kill there would.
pub const MOVE_KILL_HOOK: &str = "CODEX_CONSULT_TEST_STATE_MOVE_KILL";
/// (S2) The test hook that sets how long a process waits for the move lock, in milliseconds.
pub const LOCK_WAIT_HOOK: &str = "CODEX_CONSULT_TEST_STATE_LOCK_WAIT_MS";
/// (S2) The test hook that makes the listing of a directory of that name fail in the live-run walk
/// (test mode only), as an unreadable run directory would.
pub const UNREADABLE_HOOK: &str = "CODEX_CONSULT_TEST_STATE_UNREADABLE";

/// How long a process waits for the move lock before it reads the state again.
const LOCK_WAIT_DEFAULT_MS: u64 = 10_000;
/// A move lock older than this is taken over whatever its owner: a move takes moments.
const LOCK_STALE_SECS: u64 = 600;
/// A lock file that does not parse is taken over once it is this old (its creator died between
/// the exclusive create and the write).
const LOCK_GARBAGE_SECS: u64 = 10;

/// `<repo>/.c3`.
pub fn path(repo_root: &Path) -> PathBuf {
    repo_root.join(STATE_DIR)
}

/// `<repo>/.eck`.
pub fn legacy_path(repo_root: &Path) -> PathBuf {
    repo_root.join(LEGACY_STATE_DIR)
}

/// A directory that is itself there - not a link to one, read without following it.
fn real_dir(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir() && !is_link(&m))
}

/// The folder a read-only command reads: `.c3` when it is a real directory, else (when there is no
/// `.c3` in any form) `.eck` when that is a directory (until a writing command moved it), else
/// `None`. (S2, F36-3) A `.c3` that is a link is never read through.
pub fn read_path(repo_root: &Path) -> Option<PathBuf> {
    let c3 = path(repo_root);
    match std::fs::symlink_metadata(&c3) {
        Ok(m) if m.is_dir() && !is_link(&m) => return Some(c3),
        Ok(_) => return None,
        Err(_) => {}
    }
    let eck = legacy_path(repo_root);
    eck.is_dir().then_some(eck)
}

/// The repo-relative POSIX label of `name` inside the folder `dir` (`.c3/ROADMAP.md`).
pub fn label(dir: &Path, name: &str) -> String {
    let d = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| STATE_DIR.to_string());
    format!("{d}/{name}")
}

/// Whether two path components name the same entry, compared the way the file system compares
/// names: case-insensitively on Windows and macOS, exactly elsewhere.
pub fn names_equal(a: &str, b: &str) -> bool {
    if cfg!(any(windows, target_os = "macos")) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// Whether a path component is C3's folder or its legacy name - every exclusion that hides the
/// tool's own folder (reviewer packs, snapshots, `--diff` patches) covers both.
pub fn is_state_component(component: &str) -> bool {
    names_equal(component, STATE_DIR) || names_equal(component, LEGACY_STATE_DIR)
}

/// Which of the two folders a repository has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Layout {
    /// Neither: `c3 init` (or any writing command) creates `.c3/`.
    #[default]
    Absent,
    /// `.c3/` only.
    Current,
    /// `.eck/` only: read as the fallback until a writing command moves it.
    LegacyOnly,
    /// Both: C3 uses `.c3/`; nothing is moved or merged.
    Both,
}

/// The repository's [`Layout`]: `.c3` counts in any form, `.eck` when it is a directory.
pub fn layout(repo_root: &Path) -> Layout {
    let c3 = std::fs::symlink_metadata(path(repo_root)).is_ok();
    let eck = legacy_path(repo_root).is_dir();
    match (c3, eck) {
        (false, false) => Layout::Absent,
        (true, false) => Layout::Current,
        (false, true) => Layout::LegacyOnly,
        (true, true) => Layout::Both,
    }
}

/// (S2, F36-3) Why `<repo>/.c3` cannot be C3's folder: it is a link (a symlink, a junction or
/// another reparse point) or not a directory. `None` when it is a real directory or absent. Every
/// writing command refuses on it (nothing written, nothing created beside it); `c3 doctor` reports
/// it.
pub fn link_problem(repo_root: &Path) -> Option<String> {
    let c3 = path(repo_root);
    let m = std::fs::symlink_metadata(&c3).ok()?;
    if is_link(&m) {
        return Some(format!(
            "{STATE_DIR} is {} - C3 never writes through a link, so every writing command refuses until {STATE_DIR} is a real directory (its target is untouched)",
            link_words(&c3)
        ));
    }
    if !m.is_dir() {
        return Some(format!(
            "{STATE_DIR} exists in the repository root and is not a directory - C3 keeps its project folder there; move it away"
        ));
    }
    None
}

/// Whether C3 may move `.eck/` (default ON): `C3_STATE_MIGRATE=off` (the environment wins) or
/// `"state_migrate": false` in the C3 config keeps it where it is - then C3 creates and uses `.c3/`
/// for its own files.
pub fn migration_enabled() -> bool {
    if let Ok(v) = std::env::var(MIGRATE_ENV) {
        return !v.trim().eq_ignore_ascii_case("off");
    }
    match crate::c3config::load() {
        Ok(Some(cfg)) => cfg.state_migrate,
        _ => true,
    }
}

/// What [`ensure`] did.
#[derive(Debug, Clone, Default)]
pub struct Ensured {
    /// The folder the command writes into: `<repo>/.c3` - or `<repo>/.eck` for this run when its
    /// move had to wait ([`Ensured::deferred`]). A writer goes through [`prepare_dir`] /
    /// [`write_in`], which take a legacy path to `.c3/` once the folder has moved.
    pub dir: PathBuf,
    /// One line per move ([`migrate_legacy`], [`migrate_collab_cache`]).
    pub moved: Vec<String>,
    /// The stub manifests written (repo-relative, `.c3/ROADMAP.md`).
    pub created: Vec<String>,
    /// The folder did not exist before this call.
    pub new_folder: bool,
    /// The `.c3/` line was added to `.gitignore`.
    pub gitignore_added: bool,
    /// (S1 point 11) Why the move of `.eck/` could not be made now (a run may be live under it, the
    /// move lock is held, the rename failed): the command goes on with the legacy folder, nothing
    /// else was created.
    pub deferred: Option<String>,
}

impl Ensured {
    /// The lines a command prints about the folder: the moves, the folder or stubs created, the
    /// `.gitignore` line added. Empty when nothing happened.
    pub fn notes(&self) -> Vec<String> {
        let mut v = self.moved.clone();
        if self.new_folder {
            v.push(format!(
                "created {STATE_DIR}/ (C3's project folder) with {} stub manifest(s) - `c3 doctor` lists the [STUB] markers to resolve",
                self.created.len()
            ));
        } else if !self.created.is_empty() {
            v.push(format!(
                "wrote {} missing stub manifest(s): {}",
                self.created.len(),
                self.created.join(", ")
            ));
        }
        if self.gitignore_added {
            v.push(format!("added `{GITIGNORE_LINE}` to .gitignore"));
        }
        v
    }
}

/// Print `notes` for command `cmd`: on stdout in a text run, on stderr when stdout carries JSON.
pub fn print_notes(cmd: &str, notes: &[String], json: bool) {
    for n in notes {
        if json {
            eprintln!("{cmd}: {n}");
        } else {
            println!("{cmd}: {n}");
        }
    }
}

/// Make sure `<repo>/.c3/` exists for a command that writes into it: move an existing `.eck/` once
/// ([`migrate_legacy`]), create the folder, finish a move an S1 build left half done, move a legacy
/// cache `<collab_root>/.c3/` into it ([`migrate_collab_cache`], when the command knows its collab
/// root), write each missing stub manifest and add `.c3/` to `.gitignore` when no line ignores it.
/// (S2, F36-3) A `.c3` that is a link or not a directory is a refusal before anything is done.
/// Inside a C3 child nothing is created or moved: an existing folder is used as it is, a missing one
/// is a refusal ([`CHILD_REFUSAL`]). (S1 point 11) When the move cannot be made now nothing is
/// created: [`Ensured::dir`] is `.eck` for this run and [`Ensured::deferred`] says why - the caller
/// writes through [`write_in`] / [`prepare_dir`], never into `.c3/` blindly.
pub fn ensure(repo_root: &Path, collab_root: Option<&Path>) -> Result<Ensured, String> {
    let dir = path(repo_root);
    if let Some(why) = link_problem(repo_root) {
        return Err(why);
    }
    if crate::engines::in_c3_child() {
        return if real_dir(&dir) {
            Ok(Ensured {
                dir,
                ..Ensured::default()
            })
        } else {
            Err(CHILD_REFUSAL.to_string())
        };
    }
    let mut out = Ensured {
        dir: dir.clone(),
        ..Ensured::default()
    };
    let m = migrate_legacy(repo_root);
    out.moved = m.notes;
    if let Some(why) = m.deferred {
        out.moved.push(format!(
            "the project folder could not be moved now ({why}); using {LEGACY_STATE_DIR}/ for this run"
        ));
        out.dir = legacy_path(repo_root);
        out.deferred = Some(why);
        return Ok(out);
    }
    // a `.c3` that appeared meanwhile is held to the same rule
    if let Some(why) = link_problem(repo_root) {
        return Err(why);
    }
    // a collab root inside the legacy folder moved with it
    let collab_root = collab_root.map(|c| moved_with_folder(repo_root, c));
    let collab_root = collab_root.as_deref();
    if std::fs::symlink_metadata(&dir).is_err() {
        match std::fs::create_dir(&dir) {
            Ok(()) => out.new_folder = true,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && real_dir(&dir) => {}
            Err(e) => return Err(format!("could not create {STATE_DIR}/ ({e})")),
        }
    }
    if !real_dir(&dir) {
        return Err(link_problem(repo_root)
            .unwrap_or_else(|| format!("{STATE_DIR} is not a real directory")));
    }
    out.moved.extend(finish_half_done(repo_root));
    if let Some(c) = collab_root {
        out.moved.extend(migrate_collab_cache(repo_root, c));
    }
    for name in STUB_MANIFESTS {
        let file = format!("{name}.md");
        let p = dir.join(&file);
        if std::fs::symlink_metadata(&p).is_ok() {
            continue;
        }
        // created only when absent - never through a link, never over a file that appeared
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&p)
        {
            Ok(mut f) => {
                use std::io::Write as _;
                f.write_all(stub_content(name).as_bytes())
                    .map_err(|e| format!("write {STATE_DIR}/{file}: {e}"))?;
                out.created.push(format!("{STATE_DIR}/{file}"));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("write {STATE_DIR}/{file}: {e}")),
        }
    }
    out.gitignore_added = ensure_gitignore(repo_root)?;
    Ok(out)
}

/// The moves of `c3 doctor --fix`: [`migrate_legacy`], the finish of a half-done move and
/// [`migrate_collab_cache`], without the stubs (`c3 doctor` reports, it does not provision). The
/// caller keeps it out of a C3 child. `Err("not moved: <why>")` when the move of `.eck/` had to wait
/// (a run may be live under it, the lock is held, the rename failed); `.eck/` was not renamed then.
pub fn migrate(repo_root: &Path, collab_root: &Path) -> Result<Vec<String>, String> {
    let m = migrate_legacy(repo_root);
    if let Some(why) = m.deferred {
        return Err(format!("not moved: {why}"));
    }
    let mut v = m.notes;
    v.extend(finish_half_done(repo_root));
    v.extend(migrate_collab_cache(
        repo_root,
        &moved_with_folder(repo_root, collab_root),
    ));
    Ok(v)
}

/// A path inside `<repo>/.eck/` that the folder's move has carried to `<repo>/.c3/`: the same
/// relative path under `.c3/` when `.eck` is gone and `.c3` is a real directory; otherwise the path
/// as given. Never moves anything.
pub fn moved_with_folder(repo_root: &Path, p: &Path) -> PathBuf {
    match split_state_path(repo_root, p) {
        Some((StateName::Legacy, rest))
            if std::fs::symlink_metadata(legacy_path(repo_root)).is_err()
                && real_dir(&path(repo_root)) =>
        {
            join_rest(&path(repo_root), &rest)
        }
        _ => p.to_path_buf(),
    }
}

/// Add [`GITIGNORE_LINE`] to the repository's root `.gitignore` when no line of it ignores
/// `.c3/` (a `.c3`, `/.c3/`, `.*` ... line counts; a later negation is honoured). Returns whether
/// the line was added.
pub fn ensure_gitignore(repo_root: &Path) -> Result<bool, String> {
    let gi_path = repo_root.join(".gitignore");
    let existing = match std::fs::read_to_string(&gi_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("read .gitignore: {e}")),
    };
    if gitignore_ignores_state_dir(repo_root, &existing) {
        return Ok(false);
    }
    match ensure_gitignore_line(&existing, GITIGNORE_LINE) {
        Some(updated) => {
            std::fs::write(&gi_path, updated).map_err(|e| format!("write .gitignore: {e}"))?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Whether a root `.gitignore` body ignores the directory `.c3` at the repository root.
pub fn gitignore_ignores_state_dir(repo_root: &Path, body: &str) -> bool {
    let mut b = ignore::gitignore::GitignoreBuilder::new(repo_root);
    for line in body.lines() {
        let _ = b.add_line(None, line);
    }
    match b.build() {
        Ok(g) => g.matched(path(repo_root), true).is_ignore(),
        Err(_) => false,
    }
}

// ------------------------------------------------------------------------------ the move lock

/// What a move lock file holds (diagnostics and the stale rule).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct LockRec {
    pid: u32,
    start_time: String,
    host: String,
    created: String,
    purpose: String,
    token: String,
}

/// (S2, F36-1) The exclusive lock of the folder's move, seen by every c3 process of the repository:
/// a file created with exclusive create in the repository's git directory (git's common dir, so
/// every worktree of the repository finds it there; its name carries a hash of the worktree root),
/// or - outside a git repository - in the temp directory; never inside `.eck/` or `.c3/`. It holds
/// the owner's pid and start time and is stale when that owner is gone (the liveness rule of the
/// recovery records) or when it is older than [`LOCK_STALE_SECS`]. Released (the file removed) on
/// drop. The mover holds it from the re-read of the state through the rename; a run that starts
/// under the legacy folder holds it while it creates its records ([`begin_run`]).
#[derive(Debug)]
pub struct MoveLock {
    path: PathBuf,
    token: String,
}

impl Drop for MoveLock {
    fn drop(&mut self) {
        if read_lock(&self.path).is_some_and(|r| r.token == self.token) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

impl MoveLock {
    /// Take the lock, waiting at most the lock wait (10 s; the test hook [`LOCK_WAIT_HOOK`]).
    /// `Err` says why it was not taken (held by another process, or the file could not be created).
    pub fn acquire(repo_root: &Path, purpose: &str) -> Result<MoveLock, String> {
        Self::acquire_within(repo_root, purpose, lock_wait())
    }

    fn acquire_within(repo_root: &Path, purpose: &str, wait: Duration) -> Result<MoveLock, String> {
        let path = lock_path(repo_root);
        let deadline = Instant::now() + wait;
        loop {
            match try_create_lock(&path, purpose) {
                Ok(l) => return Ok(l),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if take_over_if_stale(&path) {
                        continue;
                    }
                    if Instant::now() >= deadline {
                        return Err(holder_words(&path));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    return Err(format!(
                        "the move lock {} could not be created ({e})",
                        path.display()
                    ))
                }
            }
        }
    }
}

/// How long a process waits for the move lock.
fn lock_wait() -> Duration {
    let ms = c3_core::test_hooks::hook(LOCK_WAIT_HOOK)
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(LOCK_WAIT_DEFAULT_MS);
    Duration::from_millis(ms)
}

/// Where the move lock of `repo_root` lives: `<git common dir>/c3-state-move-<hash>.lock`, or the
/// temp directory outside a git repository.
fn lock_path(repo_root: &Path) -> PathBuf {
    let key = std::fs::canonicalize(repo_root)
        .map(crate::providers::dunce_simplify)
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let mut key = key.to_string_lossy().to_string();
    if cfg!(any(windows, target_os = "macos")) {
        key = key.to_lowercase();
    }
    let hash = c3_core::sha256_hex(key.as_bytes());
    let name = format!("c3-state-move-{}.lock", &hash[..12]);
    match git_common_dir(repo_root) {
        Some(dir) => dir.join(name),
        None => std::env::temp_dir().join(name),
    }
}

/// The repository's git common dir, read from the files git keeps (`.git` a directory; or a `.git`
/// file naming a worktree's git dir, whose `commondir` names the common one).
fn git_common_dir(repo_root: &Path) -> Option<PathBuf> {
    let dotgit = repo_root.join(".git");
    let meta = std::fs::metadata(&dotgit).ok()?;
    let gitdir = if meta.is_dir() {
        dotgit
    } else {
        let text = std::fs::read_to_string(&dotgit).ok()?;
        let p = text
            .lines()
            .find_map(|l| l.trim().strip_prefix("gitdir:"))?
            .trim()
            .to_string();
        let p = PathBuf::from(p);
        if p.is_absolute() {
            p
        } else {
            repo_root.join(p)
        }
    };
    let common = match std::fs::read_to_string(gitdir.join("commondir")) {
        Ok(c) => {
            let c = PathBuf::from(c.trim());
            if c.is_absolute() {
                c
            } else {
                gitdir.join(c)
            }
        }
        Err(_) => gitdir,
    };
    common.is_dir().then_some(common)
}

fn read_lock(path: &Path) -> Option<LockRec> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Create the lock file by exclusive create and write this process's record into it.
fn try_create_lock(path: &Path, purpose: &str) -> std::io::Result<MoveLock> {
    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let me = std::process::id();
    let rec = LockRec {
        pid: me,
        start_time: crate::liveness::proc::process_start_iso(me).unwrap_or_default(),
        host: c3_core::host::machine_name(),
        created: chrono::Utc::now().to_rfc3339(),
        purpose: purpose.to_string(),
        token: uuid::Uuid::new_v4().to_string(),
    };
    let bytes = serde_json::to_vec(&rec).unwrap_or_default();
    if let Err(e) = f.write_all(&bytes).and_then(|()| f.flush()) {
        drop(f);
        let _ = std::fs::remove_file(path);
        return Err(e);
    }
    Ok(MoveLock {
        path: path.to_path_buf(),
        token: rec.token,
    })
}

/// The age of a file by its modification time (`None` when it cannot be read).
fn file_age_secs(path: &Path) -> Option<u64> {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    Some(
        std::time::SystemTime::now()
            .duration_since(modified)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    )
}

/// Remove a stale lock (its owner gone, older than [`LOCK_STALE_SECS`], or unparsable and older
/// than [`LOCK_GARBAGE_SECS`]) so the caller can retry the exclusive create. Returns whether the
/// caller should retry at once (taken away, or already gone).
fn take_over_if_stale(path: &Path) -> bool {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return true,
        Err(_) => return false,
    };
    let age = file_age_secs(path);
    let stale = match serde_json::from_slice::<LockRec>(&bytes) {
        Ok(rec) => {
            let here = rec.host.is_empty()
                || rec
                    .host
                    .eq_ignore_ascii_case(&c3_core::host::machine_name());
            // (a pid reused - even by this process - has another start time: gone)
            let gone = here && !crate::liveness::proc::pid_alive(rec.pid, &rec.start_time);
            gone || age.is_some_and(|a| a > LOCK_STALE_SECS)
        }
        Err(_) => age.is_none_or(|a| a > LOCK_GARBAGE_SECS),
    };
    // only the record that was judged is removed (another taker may have replaced it meanwhile)
    if stale && std::fs::read(path).ok().as_deref() == Some(&bytes[..]) {
        return std::fs::remove_file(path).is_ok();
    }
    false
}

/// Who holds the lock, in words.
fn holder_words(path: &Path) -> String {
    match read_lock(path) {
        Some(r) => format!(
            "the move lock is held by c3 pid {} on {} ({}) since {}",
            r.pid, r.host, r.purpose, r.created
        ),
        None => "the move lock is held by another c3 process".to_string(),
    }
}

// ------------------------------------------------------------------------------ the move

/// What [`migrate_legacy`] did.
#[derive(Debug, Clone, Default)]
pub struct Migration {
    /// `.eck/` was renamed to `.c3/`.
    pub moved: bool,
    /// One line per thing done or left.
    pub notes: Vec<String>,
    /// (S1 point 11) The move was due but could not be made now - a run may be live under `.eck/`,
    /// the move lock is held, a leftover could not be relocated, or the rename failed: why. `.eck/`
    /// was not renamed; the caller goes on with it for this run.
    pub deferred: Option<String>,
}

/// Move an existing `<repo>/.eck` to `<repo>/.c3` - once: only when `.c3` does not exist in any
/// form, `.eck` is a real directory (a link is refused and left untouched), and the switch allows
/// it ([`migration_enabled`]). (S2, F36-1) Under the [`MoveLock`]: the state is read again once the
/// lock is held (another process may have moved it), and a process that cannot take the lock in
/// the bounded wait reads it again too - `.c3/` there and `.eck/` gone means the move happened.
/// (S2, F36-6) Not while a run may be live under `.eck/` ([`live_run_under`], fail closed, checked
/// under the lock immediately before the rename). (S2, F36-2) The old tool's leftovers at the top
/// of the folder go to `legacy/` and its report `lastsnapshot/AnswerToSA.md` becomes
/// `last-report.md` INSIDE `.eck/` first (each step idempotent), and the rename of the folder is the
/// last step - so whatever is interrupted, a `.c3/` never holds the old tool's `anchor` at its top;
/// a leftover that cannot be relocated stops the move. Every rename error is followed by the same
/// re-read before anything is concluded. Anything that stops the move is reported as
/// [`Migration::deferred`] ("not now"). After the move `.c3/` is added to `.gitignore` (the `.eck/`
/// line is left alone). The caller keeps it out of the hook and of a C3 child.
pub fn migrate_legacy(repo_root: &Path) -> Migration {
    let mut out = Migration::default();
    let c3 = path(repo_root);
    let eck = legacy_path(repo_root);
    if NO_MOVE.load(std::sync::atomic::Ordering::SeqCst) || std::fs::symlink_metadata(&c3).is_ok() {
        return out;
    }
    let Ok(meta) = std::fs::symlink_metadata(&eck) else {
        return out;
    };
    if !migration_enabled() {
        return out;
    }
    if is_link(&meta) {
        out.notes.push(legacy_link_note(&eck));
        return out;
    }
    if !meta.is_dir() {
        return out;
    }
    let lock = match MoveLock::acquire(repo_root, "move") {
        Ok(l) => l,
        Err(why) => return after_failure(repo_root, out, why),
    };
    // the state again, under the lock: another process may have moved it meanwhile
    if std::fs::symlink_metadata(&c3).is_ok() {
        return out;
    }
    match std::fs::symlink_metadata(&eck) {
        Ok(m) if is_link(&m) => {
            out.notes.push(legacy_link_note(&eck));
            return out;
        }
        Ok(m) if m.is_dir() => {}
        _ => return out,
    }
    if let Some(why) = live_run_under(repo_root, &eck) {
        out.deferred = Some(why);
        return out;
    }
    hold_inside_the_move();

    // The old tool's leftovers and report, relocated inside `.eck/` before the rename.
    let rel = relocate_leftovers(&eck, LEGACY_STATE_DIR);
    if let Some(why) = rel.blocked.clone() {
        out.notes.extend(rel.relocated_notes(LEGACY_STATE_DIR));
        out.notes.extend(rel.left);
        out.deferred = Some(why);
        return out;
    }
    if c3_core::test_hooks::hook(MOVE_KILL_HOOK).is_some() {
        std::process::exit(86);
    }
    let renamed = if c3_core::test_hooks::hook(RENAME_FAIL_HOOK).is_some() {
        Err(std::io::Error::other(format!(
            "simulated by {RENAME_FAIL_HOOK}"
        )))
    } else {
        std::fs::rename(&eck, &c3)
    };
    if let Err(e) = renamed {
        drop(lock);
        out.notes.extend(rel.relocated_notes(LEGACY_STATE_DIR));
        out.notes.extend(rel.left);
        return after_failure(
            repo_root,
            out,
            format!(
                "the rename of {LEGACY_STATE_DIR}/ to {STATE_DIR}/ failed: {e}; nothing else was tried"
            ),
        );
    }
    out.moved = true;
    out.notes.push(format!(
        "moved {LEGACY_STATE_DIR}/ to {STATE_DIR}/ - C3's project folder is now {STATE_DIR}/"
    ));
    out.notes.extend(rel.relocated_notes(STATE_DIR));
    out.notes.extend(rel.left.iter().map(|l| relabel(l)));
    match ensure_gitignore(repo_root) {
        Ok(true) => out.notes.push(format!(
            "added `{GITIGNORE_LINE}` to .gitignore (the `{LEGACY_STATE_DIR}/` line is left as it is)"
        )),
        Ok(false) => {}
        Err(e) => out
            .notes
            .push(format!("`{GITIGNORE_LINE}` not added to .gitignore: {e}")),
    }
    drop(lock);
    out
}

/// The note for a `.eck` that is a link.
fn legacy_link_note(eck: &Path) -> String {
    format!(
        "{LEGACY_STATE_DIR} is {} - not moved to {STATE_DIR}/ (C3 never moves a folder through a link); its target is untouched and a fresh {STATE_DIR}/ is used",
        link_words(eck)
    )
}

/// A note about `.eck/<x>` written before the rename, as it reads after it (`.c3/<x>`).
fn relabel(note: &str) -> String {
    note.replace(&format!("{LEGACY_STATE_DIR}/"), &format!("{STATE_DIR}/"))
}

/// (S2, F36-1) What a mover concludes when it could not take the lock or its rename failed: it reads
/// the state again first. `.c3` there (and `.eck` gone) - the move happened, or another `.c3`
/// exists: no deferral, the caller uses `.c3/`. `.eck` still a directory: deferred for `why`.
fn after_failure(repo_root: &Path, mut out: Migration, why: String) -> Migration {
    // `.c3` there: the move happened (or, with `.eck` still there too, both folders exist - nothing
    // is merged, C3 uses `.c3/` and `c3 doctor` says so)
    if std::fs::symlink_metadata(path(repo_root)).is_ok() {
        return out;
    }
    if std::fs::symlink_metadata(legacy_path(repo_root)).is_ok() {
        out.deferred = Some(why);
    }
    out
}

/// The test hook [`MOVE_HOLD_HOOK`]: hold this mover inside the move until the test releases it.
fn hold_inside_the_move() {
    let Some(v) = c3_core::test_hooks::hook(MOVE_HOLD_HOOK) else {
        return;
    };
    let release = PathBuf::from(v.trim());
    let mut held = release.clone().into_os_string();
    held.push(".held");
    let _ = std::fs::write(PathBuf::from(held), std::process::id().to_string());
    let deadline = Instant::now() + Duration::from_secs(60);
    while !release.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// What [`relocate_leftovers`] did.
#[derive(Debug, Default)]
struct Relocation {
    /// The leftovers moved into `legacy/`.
    moved: Vec<String>,
    /// The report became `last-report.md`.
    report: bool,
    /// One line per thing left (labelled with the folder they were left in).
    left: Vec<String>,
    /// A leftover is still at the top of the folder: why (the folder's rename must not follow).
    blocked: Option<String>,
}

impl Relocation {
    /// The lines for what was moved, with the folder named `label` (where it is now).
    fn relocated_notes(&self, label: &str) -> Vec<String> {
        let mut v = Vec::new();
        if !self.moved.is_empty() {
            v.push(format!(
                "moved the old tool's {} to {label}/{LEGACY_SUBDIR}/ (C3 never reads them)",
                self.moved.join(", ")
            ));
        }
        if self.report {
            v.push(format!(
                "moved {label}/{}/{} to {label}/{LAST_REPORT}",
                LEGACY_REPORT[0], LEGACY_REPORT[1]
            ));
        }
        v
    }
}

/// (S2, F36-2/F36-3) Relocate the old tool's leftovers at the top of `folder` into
/// `folder/legacy/` and its report `folder/lastsnapshot/AnswerToSA.md` to `folder/last-report.md`.
/// Each step is idempotent (a step already made is skipped; a destination name that exists already
/// keeps its bytes and the leftover goes to the first free `<name>.<n>`), and none goes through a
/// link: a `legacy` or `lastsnapshot` that is a link stops that step and is reported. `label` names
/// the folder in the notes.
fn relocate_leftovers(folder: &Path, label: &str) -> Relocation {
    let mut out = Relocation::default();
    let legacy = folder.join(LEGACY_SUBDIR);
    for name in LEGACY_LEFTOVERS {
        let src = folder.join(name);
        if std::fs::symlink_metadata(&src).is_err() {
            continue;
        }
        let ready = match std::fs::symlink_metadata(&legacy) {
            Err(_) => std::fs::create_dir(&legacy)
                .or_else(|e| if real_dir(&legacy) { Ok(()) } else { Err(e) })
                .map_err(|e| format!("could not create {label}/{LEGACY_SUBDIR}/ ({e})")),
            Ok(m) if m.is_dir() && !is_link(&m) => Ok(()),
            Ok(m) => Err(format!(
                "{label}/{LEGACY_SUBDIR} is {} - never written through",
                if is_link(&m) {
                    link_words(&legacy)
                } else {
                    "not a plain directory"
                }
            )),
        };
        let result = ready.and_then(|()| {
            let mut dst = legacy.join(name);
            let mut n = 1;
            while std::fs::symlink_metadata(&dst).is_ok() {
                dst = legacy.join(format!("{name}.{n}"));
                n += 1;
            }
            std::fs::rename(&src, &dst)
                .map_err(|e| format!("could not move it to {label}/{LEGACY_SUBDIR}/ ({e})"))
        });
        match result {
            Ok(()) => out.moved.push(name.to_string()),
            Err(why) => {
                out.left.push(format!("left {label}/{name}: {why}"));
                out.blocked.get_or_insert_with(|| {
                    format!("the old tool's {name} could not be moved to {label}/{LEGACY_SUBDIR}/ - {why}; the folder was not renamed")
                });
            }
        }
    }

    // The old tool's report becomes `last-report.md` - never taken through a linked `lastsnapshot`.
    let snap = folder.join(LEGACY_REPORT[0]);
    match std::fs::symlink_metadata(&snap) {
        Ok(m) if is_link(&m) => out.left.push(format!(
            "left {label}/{}: it is {} - nothing is taken through it",
            LEGACY_REPORT[0],
            link_words(&snap)
        )),
        Ok(m) if m.is_dir() => {
            let report = snap.join(LEGACY_REPORT[1]);
            if std::fs::symlink_metadata(&report).is_ok_and(|m| m.is_file() && !is_link(&m)) {
                let dst = folder.join(LAST_REPORT);
                if std::fs::symlink_metadata(&dst).is_ok() {
                    out.left.push(format!(
                        "left {label}/{}/{}: {label}/{LAST_REPORT} exists already",
                        LEGACY_REPORT[0], LEGACY_REPORT[1]
                    ));
                } else {
                    match std::fs::rename(&report, &dst) {
                        Ok(()) => out.report = true,
                        Err(e) => out.left.push(format!(
                            "left {label}/{}/{}: could not move it to {label}/{LAST_REPORT} ({e})",
                            LEGACY_REPORT[0], LEGACY_REPORT[1]
                        )),
                    }
                }
            }
        }
        _ => {}
    }
    out
}

/// (S2, F36-2) Finish a move an S1 build left half done: a `.c3/` that still has the old tool's
/// `update_seq` or `telemetry_queue.json` at its top gets them - and an `anchor` beside them, the old
/// tool's too - moved into `.c3/legacy/`, and `lastsnapshot/AnswerToSA.md` becomes `last-report.md`,
/// under the move lock. Nothing happens for a `.c3/` without those marks (C3's own `anchor` stays).
fn finish_half_done(repo_root: &Path) -> Vec<String> {
    let c3 = path(repo_root);
    let marked = |c3: &Path| {
        real_dir(c3)
            && HALF_DONE_MARKERS
                .iter()
                .any(|n| std::fs::symlink_metadata(c3.join(n)).is_ok())
    };
    if !marked(&c3) {
        return Vec::new();
    }
    let _lock = match MoveLock::acquire(repo_root, "finish a half-done move") {
        Ok(l) => l,
        Err(why) => {
            return vec![format!(
                "the old tool's files at the top of {STATE_DIR}/ were not moved now ({why})"
            )]
        }
    };
    if !marked(&c3) {
        return Vec::new();
    }
    let rel = relocate_leftovers(&c3, STATE_DIR);
    let mut v: Vec<String> = rel
        .relocated_notes(STATE_DIR)
        .into_iter()
        .map(|n| format!("finished the move an earlier C3 left half done: {n}"))
        .collect();
    v.extend(rel.left);
    v
}

// ------------------------------------------------------------------------------ the live-run guard

/// (S1 point 11, S2 F36-6) Why the legacy folder may not be moved now: a run may be live in a task
/// directory under `dir` (searched three levels deep) - a detached consultation (its status
/// records), a consultation holding its task's ownership lock, an interrupted consultation (an
/// active recovery record) or a work run (its `run.json` with a live owner). FAIL CLOSED: a
/// directory that cannot be listed, a record that cannot be read and an owner whose start time
/// cannot be read all mean "a run may be live", and so does a directory behind a link (never walked,
/// so what runs there cannot be seen). `None` only when the whole walk saw nothing live.
pub fn live_run_under(repo_root: &Path, dir: &Path) -> Option<String> {
    walk_runs(repo_root, dir, 3)
}

fn walk_runs(repo_root: &Path, d: &Path, depth: usize) -> Option<String> {
    let may = |what: String| Some(format!("{what} - a run may be live there"));
    let rd = match read_dir_checked(d) {
        Ok(rd) => rd,
        Err(e) => return may(format!("{} cannot be listed ({e})", shown(repo_root, d))),
    };
    let mut subdirs: Vec<(String, PathBuf)> = Vec::new();
    for entry in rd {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                return may(format!(
                    "an entry of {} cannot be read ({e})",
                    shown(repo_root, d)
                ))
            }
        };
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let m = match std::fs::symlink_metadata(&p) {
            Ok(m) => m,
            Err(e) => return may(format!("{} cannot be read ({e})", shown(repo_root, &p))),
        };
        if is_link(&m) {
            if NOT_RUN_PLACES.contains(&name.as_str()) {
                continue;
            }
            // a directory behind a link is never walked - what runs there cannot be seen
            if std::fs::metadata(&p).map(|t| t.is_dir()).unwrap_or(true) {
                return may(format!(
                    "{} is {} and the runs behind it are not checked",
                    shown(repo_root, &p),
                    link_words(&p)
                ));
            }
            continue;
        }
        if m.is_dir() {
            subdirs.push((name, p));
        }
    }
    subdirs.sort();
    for (name, task_dir) in subdirs {
        if let Some(why) = task_may_be_live(repo_root, &task_dir) {
            return Some(why);
        }
        // not a place where task directories live
        let skip = NOT_RUN_PLACES.contains(&name.as_str()) || name == "work" || name == "handoffs";
        if depth > 1 && !skip {
            if let Some(why) = walk_runs(repo_root, &task_dir, depth - 1) {
                return Some(why);
            }
        }
    }
    None
}

/// `read_dir`, with the test hook [`UNREADABLE_HOOK`] (a directory of that name cannot be listed).
fn read_dir_checked(d: &Path) -> std::io::Result<std::fs::ReadDir> {
    if let Some(name) = c3_core::test_hooks::hook(UNREADABLE_HOOK) {
        let hit = d
            .file_name()
            .is_some_and(|n| names_equal(&n.to_string_lossy(), name.trim()));
        if hit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("access is denied (test hook {UNREADABLE_HOOK})"),
            ));
        }
    }
    std::fs::read_dir(d)
}

/// Whether one task directory may hold a live run (see [`live_run_under`]): `Some(why)` when it
/// does or cannot be told.
fn task_may_be_live(repo_root: &Path, task_dir: &Path) -> Option<String> {
    let shown_t = shown(repo_root, task_dir);
    let live = || Some(format!("a run is live under {shown_t}"));
    let unknown = |what: String| Some(format!("{what} - a run may be live under {shown_t}"));
    if crate::consult::detach::detached_run_may_be_live(task_dir) {
        return live();
    }
    // the task directory's own recovery records (a kill copy in the codex home holds nothing open
    // under the folder)
    let pending = crate::liveness::pending::pending_paths_with(task_dir, None);
    if let Some(e) = pending.error {
        return unknown(format!("its recovery records cannot be listed ({e})"));
    }
    for p in pending.paths {
        let rd = crate::liveness::pending::read_pending_file(&p);
        match rd.record {
            Some(record) => {
                if crate::liveness::pending::test_pending_active(&record, &p).active {
                    return live();
                }
            }
            None if rd.exists => {
                return unknown(format!(
                    "the recovery record {} cannot be read ({})",
                    shown(repo_root, &p),
                    rd.error.unwrap_or_default()
                ))
            }
            None => {}
        }
    }
    let work_root = task_dir.join("work");
    match crate::work::record::run_dirs(&work_root) {
        Err(e) => return unknown(e),
        Ok(dirs) => {
            for (_, run_dir) in dirs {
                match crate::work::record::read_record(&run_dir) {
                    Err(e) => return unknown(e),
                    Ok(Some(rec)) if work_run_may_be_live(&rec) => return live(),
                    Ok(_) => {}
                }
            }
        }
    }
    match ownership_lock_held(task_dir) {
        Ok(true) => live(),
        Ok(false) => None,
        Err(e) => unknown(format!("its ownership lock cannot be checked ({e})")),
    }
}

/// A work run's record names a live owner - or one whose identity cannot be read (fail closed).
fn work_run_may_be_live(rec: &crate::work::record::RunRecord) -> bool {
    if crate::work::record::is_live(rec) {
        return true;
    }
    crate::work::record::is_pending(&rec.status)
        && rec
            .pid
            .is_some_and(|p| p > 0 && owner_cannot_be_read(p as u32))
}

/// (S2, F36-6) A process that exists but whose start time cannot be read. On Windows the liveness
/// rule already reads that as alive (access denied gives an empty start time); on Linux an
/// unreadable `/proc/<pid>/stat` reads as "no such process", so the pid's own entry decides.
#[cfg(unix)]
fn owner_cannot_be_read(pid: u32) -> bool {
    if crate::liveness::proc::process_start_iso(pid).is_some() {
        return false;
    }
    if !Path::new("/proc/self").exists() {
        return false;
    }
    match std::fs::symlink_metadata(format!("/proc/{pid}")) {
        Ok(_) => true,
        Err(e) => e.kind() != std::io::ErrorKind::NotFound,
    }
}

#[cfg(not(unix))]
fn owner_cannot_be_read(_pid: u32) -> bool {
    false
}

/// Whether a consultation holds the task's ownership lock (`.consult.lock`, owned by holding it
/// open): probed by opening it the way its owner does, never created. Only called under the move
/// lock, which no run of the legacy folder holds while it takes its task lock ([`begin_run`]).
fn ownership_lock_held(task_dir: &Path) -> Result<bool, String> {
    let p = task_dir.join(c3_core::store::OWNERSHIP_LOCK);
    match std::fs::symlink_metadata(&p) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.to_string()),
        Ok(m) if is_link(&m) || !m.is_file() => {
            return Err(format!("{} is not a plain file", p.display()))
        }
        Ok(_) => {}
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ)
            .open(&p)
        {
            Ok(_) => Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => match e.raw_os_error() {
                // ERROR_SHARING_VIOLATION / ERROR_LOCK_VIOLATION: held by its consultation
                Some(32) | Some(33) => Ok(true),
                _ => Err(e.to_string()),
            },
        }
    }
    #[cfg(not(windows))]
    {
        let f = match std::fs::OpenOptions::new().read(true).write(true).open(&p) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.to_string()),
        };
        match f.try_lock() {
            Ok(()) => Ok(false),
            Err(std::fs::TryLockError::WouldBlock) => Ok(true),
            Err(std::fs::TryLockError::Error(e)) => Err(e.to_string()),
        }
    }
}

// ------------------------------------------------------------------------------ run starts

/// (S2, F36-6) Held by a consultation or work run that starts with its records under the legacy
/// folder, while it creates them (dropped right after its first live record is written).
#[derive(Debug)]
pub struct RunStart {
    _lock: Option<MoveLock>,
}

/// (S2, F36-6) Before a consultation or work run creates its records under `collab_root`: when that
/// lies in the legacy folder `.eck/`, take the move lock (a bounded wait) and check under it that
/// `.eck/` is still the folder - so the run cannot begin between a mover's live-run check and its
/// rename (on Unix an open file does not stop a rename; this does). Elsewhere, and inside a C3 child
/// (which never moves the folder), nothing is taken. `Err` (nothing was started) when the lock
/// cannot be taken or the folder moved to `.c3/` while the run was being prepared.
pub fn begin_run(repo_root: &Path, collab_root: &Path) -> Result<RunStart, String> {
    let none = RunStart { _lock: None };
    if !matches!(
        split_state_path(repo_root, collab_root),
        Some((StateName::Legacy, _))
    ) || crate::engines::in_c3_child()
    {
        return Ok(none);
    }
    let eck = legacy_path(repo_root);
    if !real_dir(&eck) && !real_dir(&path(repo_root)) {
        // no folder at all: nothing a mover could take away
        return Ok(none);
    }
    let lock = MoveLock::acquire(repo_root, "run start").map_err(|why| {
        format!(
            "the project folder {LEGACY_STATE_DIR}/ is being moved by another c3 process ({why}); nothing was started - try again"
        )
    })?;
    if !real_dir(&eck) {
        return Err(format!(
            "the project folder moved from {LEGACY_STATE_DIR}/ to {STATE_DIR}/ while this run was being prepared; nothing was started - run it again (its records go to {})",
            shown(repo_root, &moved_with_folder(repo_root, collab_root))
        ));
    }
    Ok(RunStart { _lock: Some(lock) })
}

// ------------------------------------------------------------------------------ collab paths

/// Which of the two folders a path starts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StateName {
    Current,
    Legacy,
}

/// (S2, F36-5) `p` (a relative path against `repo_root`) as a repo-relative `/`-separated path with
/// `.` and `..` resolved lexically; `None` when it does not lie in the repository. When the text
/// does not show it in the repository, the canonical forms are compared (a CI runner's 8.3 short
/// TEMP path, a temp directory reached through a link).
fn repo_relative_normal(repo_root: &Path, p: &Path) -> Option<String> {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        repo_root.join(p)
    };
    let abs = lexical(&abs);
    let root = lexical(repo_root);
    if let Some(r) = c3_core::paths::repo_relative(&root, &abs) {
        return Some(r);
    }
    let croot = canonical_prefix(&root)?;
    let cabs = canonical_prefix(&abs)?;
    c3_core::paths::repo_relative(&croot, &cabs)
}

/// `p` with `.` dropped and `..` taking the previous component away (never above the root).
fn lexical(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The canonical form of the longest existing ancestor of `p`, with the rest appended.
fn canonical_prefix(p: &Path) -> Option<PathBuf> {
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = p.to_path_buf();
    loop {
        if let Ok(c) = std::fs::canonicalize(&cur) {
            let mut out = crate::providers::dunce_simplify(c);
            for part in rest.iter().rev() {
                out.push(part);
            }
            return Some(out);
        }
        rest.push(cur.file_name()?.to_os_string());
        if !cur.pop() {
            return None;
        }
    }
}

/// The folder a path lies in (`.c3` or `.eck` at the repository root, compared the way the file
/// system compares names) and the rest of the path below it (`/`-separated). (S2, F36-5) The path
/// is normalised first ([`repo_relative_normal`]): `sub/../.eck/collab`, `./.eck/collab`,
/// `.c3/../.eck/collab` and the absolute form all lie in `.eck`.
fn split_state_path(repo_root: &Path, p: &Path) -> Option<(StateName, String)> {
    let rel = repo_relative_normal(repo_root, p)?;
    let mut parts = rel.split('/').filter(|s| !s.is_empty() && *s != ".");
    let first = parts.next()?;
    let kind = if names_equal(first, STATE_DIR) {
        StateName::Current
    } else if names_equal(first, LEGACY_STATE_DIR) {
        StateName::Legacy
    } else {
        return None;
    };
    Some((kind, parts.collect::<Vec<_>>().join("/")))
}

/// `base` joined with a `/`-separated rest.
fn join_rest(base: &Path, rest: &str) -> PathBuf {
    let mut out = base.to_path_buf();
    for part in rest.split('/').filter(|s| !s.is_empty()) {
        out.push(part);
    }
    out
}

/// Set once by the SessionStart hook: no command of this process moves the folder.
static NO_MOVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The SessionStart hook calls this first: a collab path it resolves never moves the folder.
pub fn forbid_moves() {
    NO_MOVE.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// The lines [`note_once`] already printed in this process.
static NOTED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Print `line` to stderr once per process (`c3: <line>`); never in the hook (its stdout is the
/// session's context, its stderr nobody reads).
fn note_once(line: String) {
    if NO_MOVE.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    if let Ok(mut seen) = NOTED.lock() {
        if seen.contains(&line) {
            return;
        }
        eprintln!("c3: {line}");
        seen.push(line);
    }
}

/// (S1 point 11, S2 F36-4) The collab directory a command uses, from the path it was handed
/// (`given`, already joined to the repository root when it was relative) - NEVER moving anything.
/// This is the default every command resolves its collab directory through
/// ([`crate::providers::resolve_collab_root`], `c3 work`'s paths): CLI `--collab-dir`, the MCP
/// `collab_dir`, the hook's look for detached runs, `c3 providers`' ledger scan. Only a real
/// consultation or work start uses [`collab_dir_moving`].
///
/// (S2, F36-5) A path is normalised before it is classified (`.` and `..` resolved lexically, an
/// absolute path in the repository made relative, names compared the way the file system does):
///
/// * a path inside `.eck/` that does not exist while `.c3` does (or neither folder exists - a
///   collab path never creates a new `.eck/`): the same relative path under `.c3/`, said once;
/// * a path inside `.c3/` while `.c3` does not exist and `.eck/` does: the same relative path in
///   `.eck/`, silently (the folder has not moved);
/// * a path inside `.eck/` while `.eck/` exists and `.c3` does not: as given (normalised);
/// * with the switch off ([`migration_enabled`]): as given (normalised) - a collab directory the
///   caller places under `.eck/` is used there;
/// * every other path: exactly as given.
pub fn collab_dir(repo_root: &Path, given: PathBuf) -> PathBuf {
    resolve_collab(repo_root, given, false)
}

/// (S2, F36-4) [`collab_dir`] for a caller that starts a real consultation or work run (and so writes
/// into the collab directory): a path inside `.c3/` while `.c3` does not exist and `.eck/` does
/// makes the one-time move first ([`migrate_legacy`]; never by the hook, never inside a C3 child);
/// when it cannot be made now the legacy path is used for this run and said once. Every other case
/// is [`collab_dir`]'s.
pub fn collab_dir_moving(repo_root: &Path, given: PathBuf) -> PathBuf {
    resolve_collab(repo_root, given, true)
}

fn resolve_collab(repo_root: &Path, given: PathBuf, may_move: bool) -> PathBuf {
    let Some((kind, rest)) = split_state_path(repo_root, &given) else {
        return given;
    };
    let c3 = path(repo_root);
    let eck = legacy_path(repo_root);
    let normal = match kind {
        StateName::Current => join_rest(&c3, &rest),
        StateName::Legacy => join_rest(&eck, &rest),
    };
    if !migration_enabled() {
        return normal;
    }
    let c3_exists = std::fs::symlink_metadata(&c3).is_ok();
    match kind {
        StateName::Legacy => {
            if std::fs::symlink_metadata(&normal).is_ok() || (!c3_exists && eck.is_dir()) {
                return normal;
            }
            let mapped = join_rest(&c3, &rest);
            let why = if c3_exists {
                "since the project folder moved"
            } else {
                "(C3's project folder is .c3/)"
            };
            note_once(format!(
                "collab directory {} is {} {why}",
                shown(repo_root, &given),
                shown(repo_root, &mapped)
            ));
            mapped
        }
        StateName::Current => {
            if c3_exists || !eck.is_dir() {
                return normal;
            }
            let legacy = join_rest(&eck, &rest);
            if !may_move || NO_MOVE.load(std::sync::atomic::Ordering::SeqCst) {
                return legacy;
            }
            let why = if crate::engines::in_c3_child() {
                "inside a C3 child".to_string()
            } else {
                let m = migrate_legacy(repo_root);
                for n in &m.notes {
                    note_once(n.clone());
                }
                match m.deferred {
                    // moved (by this process or another) - or a link left alone, whose fresh `.c3/`
                    // is used
                    None => return normal,
                    Some(why) => why,
                }
            };
            note_once(format!(
                "the project folder could not be moved now ({why}); using {} for this run",
                shown(repo_root, &legacy)
            ));
            legacy
        }
    }
}

/// Whether two paths name the same directory (canonically; the paths as given when one cannot be
/// canonicalised).
fn same_dir(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// How a path is named in a note: repo-relative when it lies in the repository, else as given.
fn shown(repo_root: &Path, p: &Path) -> String {
    match c3_core::paths::repo_relative(repo_root, p) {
        Some(r) if !r.is_empty() => r,
        _ => p.display().to_string(),
    }
}

// ------------------------------------------------------------------------------ writes

/// The refusal for a link met on a write path.
fn link_refusal(repo_root: &Path, p: &Path) -> String {
    format!(
        "{} is {} - C3 never writes through a link; nothing was written",
        shown(repo_root, p),
        link_words(p)
    )
}

/// (S2, F36-1/F36-3) The directory `dir` made ready for a write NOW. A path inside `.eck/` that the
/// folder's move has carried to `.c3/` is taken there ([`moved_with_folder`]: a writer works from
/// the resolved state, never from a stale path). For a path inside either folder, the folder itself
/// must be a real directory - a link is refused, and a legacy folder that is gone is never created
/// again (its path is resolved once more, then it is an error) - and each missing directory below it
/// is created one level at a time, never through a link (an existing component that is a link is
/// refused). A path outside both folders is created as it is. Returns the directory to write into.
pub fn prepare_dir(repo_root: &Path, dir: &Path) -> Result<PathBuf, String> {
    for attempt in 0..2 {
        let p = moved_with_folder(repo_root, dir);
        let Some((kind, rest)) = split_state_path(repo_root, &p) else {
            std::fs::create_dir_all(&p).map_err(|e| format!("create {}: {e}", p.display()))?;
            return Ok(p);
        };
        let root = match kind {
            StateName::Current => path(repo_root),
            StateName::Legacy => legacy_path(repo_root),
        };
        match std::fs::symlink_metadata(&root) {
            Ok(m) if is_link(&m) => return Err(link_refusal(repo_root, &root)),
            Ok(m) if m.is_dir() => {}
            Ok(_) => {
                return Err(format!(
                    "{} is not a directory - nothing was written",
                    shown(repo_root, &root)
                ))
            }
            Err(_) if kind == StateName::Current => match std::fs::create_dir(&root) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && real_dir(&root) => {}
                Err(e) => return Err(format!("create {STATE_DIR}/: {e}")),
            },
            Err(_) if attempt == 0 => continue,
            Err(_) => {
                return Err(format!(
                    "the project folder {LEGACY_STATE_DIR}/ is gone and {STATE_DIR}/ is not there - nothing was written (C3 never creates {LEGACY_STATE_DIR}/ again)"
                ))
            }
        }
        let mut cur = root;
        let mut vanished = false;
        for part in rest.split('/').filter(|s| !s.is_empty()) {
            cur.push(part);
            match std::fs::symlink_metadata(&cur) {
                Ok(m) if is_link(&m) => return Err(link_refusal(repo_root, &cur)),
                Ok(m) if m.is_dir() => {}
                Ok(_) => {
                    return Err(format!(
                        "{} is not a directory - nothing was written",
                        shown(repo_root, &cur)
                    ))
                }
                Err(_) => match std::fs::create_dir(&cur) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && real_dir(&cur) => {}
                    // the folder was renamed under this writer: resolve it again
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        vanished = true;
                        break;
                    }
                    Err(e) => return Err(format!("create {}: {e}", shown(repo_root, &cur))),
                },
            }
        }
        if !vanished {
            return Ok(cur);
        }
    }
    Err(format!(
        "{} could not be prepared: the project folder moved while it was written - nothing was written",
        shown(repo_root, dir)
    ))
}

/// (S2, F36-1/F36-3) Write the file `name` in the folder directory `dir` (prepared by
/// [`prepare_dir`]): its bytes come from `content`, called with the file's path just before the
/// write (so a read-modify-write reads the file it writes). A file that is a link is refused. When
/// the legacy folder was renamed between the preparation and the write, the write is made again at
/// the folder's new place. Returns the file written.
pub fn write_in(
    repo_root: &Path,
    dir: &Path,
    name: &str,
    content: impl Fn(&Path) -> Result<Vec<u8>, String>,
) -> Result<PathBuf, String> {
    let mut last = String::new();
    for _ in 0..2 {
        let d = prepare_dir(repo_root, dir)?;
        let f = d.join(name);
        if std::fs::symlink_metadata(&f).is_ok_and(|m| is_link(&m)) {
            return Err(link_refusal(repo_root, &f));
        }
        let bytes = content(&f)?;
        match std::fs::write(&f, &bytes) {
            Ok(()) => return Ok(f),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                last = format!("write {}: {e}", shown(repo_root, &f));
            }
            Err(e) => return Err(format!("write {}: {e}", shown(repo_root, &f))),
        }
    }
    Err(last)
}

// ------------------------------------------------------------------------------ the collab cache

/// The first link among the components of `p`: those below the repository root when `p` lies in
/// it, else `p` and its parent (a collab root outside the repository and its `.c3`).
fn first_link_in(repo_root: &Path, p: &Path) -> Option<PathBuf> {
    let is_l = |q: &Path| std::fs::symlink_metadata(q).is_ok_and(|m| is_link(&m));
    match repo_relative_normal(repo_root, p) {
        Some(rel) => {
            let mut cur = repo_root.to_path_buf();
            for part in rel.split('/').filter(|s| !s.is_empty()) {
                cur.push(part);
                if is_l(&cur) {
                    return Some(cur);
                }
            }
            None
        }
        None => [p.parent(), Some(p)]
            .into_iter()
            .flatten()
            .find(|q| is_l(q))
            .map(Path::to_path_buf),
    }
}

/// Move a legacy cache `<collab_root>/.c3/` (C3's `anchor`, `snapshot_seq`, `index/`, ...) into
/// `<repo>/.c3/`, entry by entry, when the destination entry does not exist: an existing
/// destination wins and the source entry is left and named in a note. Nothing moves when the
/// collab root IS the repository root (the same folder), when either folder is a link, (S2,
/// F36-3) when the collab directory is reached through a link (any component of it in the
/// repository), or inside a C3 child (the caller's rule). The emptied source folder is removed.
/// Returns the notes.
pub fn migrate_collab_cache(repo_root: &Path, collab_root: &Path) -> Vec<String> {
    let mut notes: Vec<String> = Vec::new();
    let src = collab_root.join(STATE_DIR);
    let dst = path(repo_root);
    let Ok(meta) = std::fs::symlink_metadata(&src) else {
        return notes;
    };
    if same_dir(&src, &dst) {
        return notes;
    }
    let src_label = shown(repo_root, &src);
    if let Some(link) = first_link_in(repo_root, &src) {
        notes.push(format!(
            "{} is {} - the cache under it is not moved into {STATE_DIR}/ (C3 never walks through a link)",
            shown(repo_root, &link),
            link_words(&link)
        ));
        return notes;
    }
    if is_link(&meta) || !meta.is_dir() {
        return notes;
    }
    let mut entries: Vec<std::ffi::OsString> = match std::fs::read_dir(&src) {
        Ok(rd) => rd.flatten().map(|e| e.file_name()).collect(),
        Err(_) => return notes,
    };
    entries.sort();
    if entries.is_empty() {
        let _ = std::fs::remove_dir(&src);
        return notes;
    }
    match std::fs::symlink_metadata(&dst) {
        Err(_) => {
            if let Err(e) = std::fs::create_dir(&dst) {
                notes.push(format!(
                    "{src_label} not moved: could not create {STATE_DIR}/ ({e})"
                ));
                return notes;
            }
        }
        Ok(m) if m.is_dir() && !is_link(&m) => {}
        Ok(_) => {
            notes.push(format!(
                "{src_label} not moved: {STATE_DIR} is a link or not a directory"
            ));
            return notes;
        }
    }
    let mut moved: Vec<String> = Vec::new();
    for name in entries {
        let n = name.to_string_lossy().to_string();
        let to = dst.join(&name);
        if std::fs::symlink_metadata(&to).is_ok() {
            notes.push(format!(
                "left {src_label}/{n}: {STATE_DIR}/{n} exists already (the destination wins)"
            ));
            continue;
        }
        match std::fs::rename(src.join(&name), &to) {
            Ok(()) => moved.push(n),
            Err(e) => notes.push(format!(
                "left {src_label}/{n}: could not move it into {STATE_DIR}/ ({e})"
            )),
        }
    }
    if !moved.is_empty() {
        notes.insert(
            0,
            format!(
                "moved C3's cache {src_label}/{{{}}} into {STATE_DIR}/ (one per repository now)",
                moved.join(", ")
            ),
        );
    }
    let _ = std::fs::remove_dir(&src);
    notes
}

// ------------------------------------------------------------------------------ ignore files

/// The ignore file that applies in `dir`: [`IGNORE_FILE`] when the directory has one, else
/// [`LEGACY_IGNORE_FILE`] when it has that, else none.
pub fn ignore_file_in(dir: &Path) -> Option<PathBuf> {
    let f = dir.join(IGNORE_FILE);
    if f.is_file() {
        return Some(f);
    }
    let f = dir.join(LEGACY_IGNORE_FILE);
    f.is_file().then_some(f)
}

/// The project's "never put in AI context" lists (gitignore syntax), read per directory from the
/// repository root down - in each directory its [`IGNORE_FILE`], or its [`LEGACY_IGNORE_FILE`] when
/// it has no `.c3ignore` - the deeper file deciding (a `!` line re-includes). One reader for every
/// place that honours them: discovery (packs, snapshots, fetch) and the `--diff` patch.
#[derive(Debug)]
pub struct ContextIgnore {
    root: PathBuf,
    cache: HashMap<PathBuf, Option<ignore::gitignore::Gitignore>>,
}

impl ContextIgnore {
    pub fn new(root: &Path) -> Self {
        ContextIgnore {
            root: root.to_path_buf(),
            cache: HashMap::new(),
        }
    }

    /// Whether the repo-relative POSIX path `rel` (a directory when `is_dir`) is excluded.
    pub fn excluded(&mut self, rel: &str, is_dir: bool) -> bool {
        let parts: Vec<&str> = rel.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return false;
        }
        let mut full = self.root.clone();
        for part in &parts {
            full.push(part);
        }
        let mut dir = self.root.clone();
        let mut hit = false;
        for (i, part) in parts.iter().enumerate() {
            let gi = self.cache.entry(dir.clone()).or_insert_with(|| {
                let f = ignore_file_in(&dir)?;
                let mut b = ignore::gitignore::GitignoreBuilder::new(&dir);
                let _ = b.add(&f);
                b.build().ok()
            });
            if let Some(g) = gi {
                match g.matched_path_or_any_parents(&full, is_dir) {
                    ignore::Match::Ignore(_) => hit = true,
                    ignore::Match::Whitelist(_) => hit = false,
                    ignore::Match::None => {}
                }
            }
            if i + 1 < parts.len() {
                dir.push(part);
            }
        }
        hit
    }
}

#[cfg(test)]
mod tests;
