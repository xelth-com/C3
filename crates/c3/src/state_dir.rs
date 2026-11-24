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
//!   and the rename; (0.2.5, F57-6) inside the folder its task directory and `handoffs/` (`work/`)
//!   are made through handles and a link there refuses the run ([`run_dirs`]).
//! * [`state_folder`] - (0.2.5, acceptance answer 2) the one function that creates `.c3/`: beside a
//!   real `.eck/` only under the move lock, after the state was read again under it, whatever the
//!   migration switch says - so no writer makes a `.c3/` while `.eck/` is being moved.
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
//! its own files - created under the move lock like every `.c3/` beside a real `.eck/`, so a
//! process with the switch off never splits a move another process makes; a collab directory
//! placed under `.eck/` is used as given). The SessionStart hook never moves anything
//! ([`forbid_moves`]).
//!
//! (0.2.5 R7b, F47-2) What these operations read, write and move INSIDE the repository goes
//! through directory handles ([`crate::contain`]), from the repository root opened once as an
//! anchor: `.c3/` is created and its stubs written relative to its own handle (opened without
//! following it), a writer's directory is reached one component at a time without following any
//! and its file published by a temporary file and a rename relative to that handle
//! ([`write_in`]), the old tool's leftovers are renamed between handles of `.eck/` and its
//! `legacy/`, the cache entries between handles of the two folders, and `.eck` is renamed to `.c3`
//! relative to the repository root's handle - so a directory swapped for a link after its check
//! can no longer redirect any of them outside. What stays path-based, and why: the move lock (in
//! git's directory, outside the repository's tree, created exclusively), the read-only checks that
//! decide WHETHER to act (the layout, [`link_problem`], the collab resolvers; each act is checked
//! again on the handles), the live-run walk (a read-only, fail-closed look through other modules'
//! record readers), `.gitignore` (a file the user owns at the repository root), and the files
//! callers write by path into a directory [`prepare_dir`] made ready (scouts, explains, the index
//! store, the copy into `lastsnapshot/`).

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use cap_std::fs::Dir;

use crate::contain::{self, Kind, Root, RootError, Step};
use crate::setup::init::{ensure_gitignore_line, stub_content, STUB_MANIFESTS};
use crate::setup::repo_models::{is_link, link_words, step_link_words};

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
/// (0.2.5, acceptance F57-4) The test hook that holds a creator of the move lock right after its
/// exclusive create, inside the gate (test mode only): it creates `<value>.held` and waits (at most
/// 60 s) until `<value>` exists.
pub const LOCK_CREATE_HOLD_HOOK: &str = "CODEX_CONSULT_TEST_STATE_LOCK_CREATE_HOLD";
/// (0.2.5, acceptance F57-4) The test hook that sets the garbage interval of the move lock
/// ([`LOCK_GARBAGE_SECS`]) in seconds (test mode only).
pub const LOCK_GARBAGE_HOOK: &str = "CODEX_CONSULT_TEST_STATE_LOCK_GARBAGE_SECS";

/// How long a process waits for the move lock before it reads the state again.
const LOCK_WAIT_DEFAULT_MS: u64 = 10_000;
/// A move lock older than this is taken over when its owner cannot be shown alive (another host's):
/// a move takes moments. (R7b, F48-2) A live owner's lock is never taken over by age.
const LOCK_STALE_SECS: u64 = 600;
/// A lock file that does not parse is taken over once it is this old (its creator died between
/// the exclusive create and the write - (F57-4) a live creator holds the gate through that interval,
/// so its file is never judged by this rule while it writes).
const LOCK_GARBAGE_SECS: u64 = 10;

/// The garbage interval: [`LOCK_GARBAGE_SECS`], or the test hook [`LOCK_GARBAGE_HOOK`].
fn lock_garbage_secs() -> u64 {
    c3_core::test_hooks::hook(LOCK_GARBAGE_HOOK)
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(LOCK_GARBAGE_SECS)
}

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
/// `None`. (S2, F36-3) A `.c3` that is a link is never read through; (R7b, F48-1) nor is a `.eck`
/// that is one (read without following it).
pub fn read_path(repo_root: &Path) -> Option<PathBuf> {
    let c3 = path(repo_root);
    match std::fs::symlink_metadata(&c3) {
        Ok(m) if m.is_dir() && !is_link(&m) => return Some(c3),
        Ok(_) => return None,
        Err(_) => {}
    }
    let eck = legacy_path(repo_root);
    real_dir(&eck).then_some(eck)
}

/// (R7b, F48-1/F48-4) Why the path `p` may not be used: it reaches into C3's folder through a link,
/// because `.c3` or `.eck` itself is a link (a symlink, a junction or another reparse point) and `p` lies
/// in it (by name, or - after a resolution that followed the link - in its target), or a component
/// of `p` in the repository is a link that resolves to the folder or into it (an alias). `None`
/// when no link leads into the folder. Every writer that reaches into the folder by a path refuses
/// on it ([`begin_run`]: consultations, panels, work runs); one line, nothing created.
pub fn folder_link_problem(repo_root: &Path, p: &Path) -> Option<String> {
    let abs = lexical(&if p.is_absolute() {
        p.to_path_buf()
    } else {
        repo_root.join(p)
    });
    for name in [STATE_DIR, LEGACY_STATE_DIR] {
        let folder = repo_root.join(name);
        let Ok(m) = std::fs::symlink_metadata(&folder) else {
            continue;
        };
        if !is_link(&m) {
            continue;
        }
        // (`lexical` already took the Windows spelling of each name)
        let by_name = c3_core::paths::repo_relative(&lexical(repo_root), &abs)
            .is_some_and(|r| r.split('/').next().is_some_and(|f| names_equal(f, name)));
        let by_target = match (std::fs::canonicalize(&folder), canonical_prefix(&abs)) {
            (Ok(t), Some(c)) => c.starts_with(crate::providers::dunce_simplify(t)),
            _ => false,
        };
        if by_name || by_target {
            return Some(format!(
                "{name} is {} - C3 never reads or writes through a link, so nothing under it is used (its target is untouched); make {name} a real directory or remove it",
                link_words(&folder)
            ));
        }
    }
    // an alias: a link in the repository that resolves to the folder or into it
    let rel = c3_core::paths::repo_relative(&lexical(repo_root), &abs)?;
    let folders: Vec<PathBuf> = [STATE_DIR, LEGACY_STATE_DIR]
        .iter()
        .filter_map(|n| std::fs::canonicalize(repo_root.join(n)).ok())
        .map(crate::providers::dunce_simplify)
        .collect();
    let mut cur = repo_root.to_path_buf();
    for part in rel.split('/').filter(|s| !s.is_empty()) {
        cur.push(part);
        let Ok(m) = std::fs::symlink_metadata(&cur) else {
            break;
        };
        if !is_link(&m) {
            continue;
        }
        let into = std::fs::canonicalize(&cur)
            .map(crate::providers::dunce_simplify)
            .is_ok_and(|t| folders.iter().any(|f| t.starts_with(f)));
        if into {
            return Some(format!(
                "{} is {} into C3's project folder - C3 never reads or writes through a link, so nothing under it is used",
                shown(repo_root, &cur),
                link_words(&cur)
            ));
        }
    }
    None
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
    // (R7b, F47-2) `.c3/` is created relative to the repository root's handle and opened without
    // following it (a link or a non-directory refuses); the stubs are created relative to its handle
    let root = Root::open_anchor(repo_root).map_err(|e| {
        format!(
            "could not create {STATE_DIR}/ (the repository root {})",
            e.words()
        )
    })?;
    // (0.2.5, acceptance answer 2) beside a real `.eck/` only under the move lock ([`state_folder`]):
    // the lock held by another process (a mover, with this process's switch off) means "not now" -
    // the command goes on in `.eck/` for this run and says so; nothing else is created
    let folder = match state_folder(repo_root, &root) {
        Ok((d, fresh)) => {
            out.new_folder = fresh;
            d
        }
        Err(FolderError::Held(why)) => {
            out.moved.push(format!(
                "{STATE_DIR}/ could not be created now ({why}); using {LEGACY_STATE_DIR}/ for this run"
            ));
            out.dir = legacy_path(repo_root);
            out.deferred = Some(why);
            return Ok(out);
        }
        Err(FolderError::Child) => return Err(CHILD_REFUSAL.to_string()),
        Err(FolderError::Step(Step::Create(e))) => {
            return Err(format!("could not create {STATE_DIR}/ ({e})"))
        }
        Err(FolderError::Step(_)) => {
            return Err(link_problem(repo_root)
                .unwrap_or_else(|| format!("{STATE_DIR} is not a real directory")))
        }
    };
    out.moved.extend(finish_half_done(repo_root));
    if let Some(c) = collab_root {
        out.moved.extend(migrate_collab_cache(repo_root, c));
    }
    for name in STUB_MANIFESTS {
        let file = format!("{name}.md");
        if contain::entry(&folder, OsStr::new(&file))
            .ok()
            .flatten()
            .is_some()
        {
            continue;
        }
        // created only when absent - an exclusive create relative to `.c3/`'s handle: never
        // through a link, never over a file that appeared
        let mut create = cap_std::fs::OpenOptions::new();
        create.write(true).create_new(true);
        match folder.open_with(&file, &create) {
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

/// (R7b, F48-3) The file that marks `.c3/` as finished: written LAST (an exclusive create through
/// the folder's handle) by a fresh creation, by the move once `.eck` is renamed, and by the finish of
/// an unfinished folder. A `.c3/` without it is unfinished: the next writing command relocates the
/// old tool's leftovers at its top - a lone `anchor` included - into `legacy/` before it writes the
/// marker ([`ensure`]), and no snapshot anchor is read from it.
pub const LAYOUT_MARKER: &str = "layout";
/// What the marker holds (the layout version).
const LAYOUT_BODY: &str = "c3 project folder, layout 1\n";

/// (R7b, F48-3) Whether the folder `dir` carries [`LAYOUT_MARKER`] (a regular file, read without
/// following it): only then is an `anchor` at its top C3's own.
pub fn is_finished(dir: &Path) -> bool {
    std::fs::symlink_metadata(dir.join(LAYOUT_MARKER)).is_ok_and(|m| m.is_file() && !is_link(&m))
}

/// Write [`LAYOUT_MARKER`] into the folder's handle (an exclusive create; one already there is
/// kept).
fn write_marker(folder: &Dir) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut o = cap_std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    match folder.open_with(LAYOUT_MARKER, &o) {
        Ok(mut f) => f.write_all(LAYOUT_BODY.as_bytes()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

/// (R7b, F47-2) `.c3` opened relative to the repository root's handle WITHOUT following it (a link
/// or a non-directory refuses). Never created here: [`state_folder`] is the one place that creates
/// it.
fn open_state_folder(root: &Root) -> Result<Dir, Step> {
    contain::open_child(root.dir(), OsStr::new(STATE_DIR), false)
}

/// Why [`state_folder`] gave no handle on `.c3/`.
#[derive(Debug)]
enum FolderError {
    /// `.c3` itself: a link, not a directory, or it could not be created or read.
    Step(Step),
    /// (0.2.5, acceptance answer 2) `.eck/` is a real directory and the move lock could not be taken
    /// (why): `.c3/` was not created now.
    Held(String),
    /// Inside a C3 child `.c3/` is never created ([`CHILD_REFUSAL`]).
    Child,
}

/// (R7b, F47-2/F48-3; 0.2.5, acceptance answer 2) The ONE function that creates `.c3/`: every writer
/// reaches the folder through it ([`ensure`], [`prepare_dir`] / [`write_in`], [`begin_run`] /
/// [`run_dirs`], [`migrate_collab_cache`]); the move ([`migrate_legacy`]) makes `.c3` only by its
/// rename, under the move lock. `.c3` is opened relative to the repository root's handle WITHOUT
/// following it (a link or a non-directory refuses). When it is not there it is created by this
/// rule: while `.eck/` is a real directory a fresh `.c3/` is created only under the [`MoveLock`],
/// with the state read again under it (a mover that renamed `.eck` meanwhile left a `.c3/`, which is
/// then opened, not made) - whatever the migration switch says. A mover holds that lock from its
/// re-read through its rename, so no `.c3/` can appear between them; before this, a writer with the
/// switch off created `.c3/` while another process moved `.eck/`, the rename failed on the non-empty
/// folder and the project had two folders. The lock held by another process is
/// [`FolderError::Held`] ("not now": [`ensure`] goes on in `.eck/` for that run and says so, a run
/// start refuses). When `.eck` is not a real directory (absent, or a link C3 never moves) there is
/// nothing to split and no lock is taken. Inside a C3 child nothing is created. A folder this call
/// created is fresh: it gets [`LAYOUT_MARKER`] at once, before the lock goes. Returns the handle and
/// whether it was fresh. Never called with the move lock held (it would wait for itself).
fn state_folder(repo_root: &Path, root: &Root) -> Result<(Dir, bool), FolderError> {
    match contain::entry(root.dir(), OsStr::new(STATE_DIR)) {
        Ok(Some(_)) => {
            return open_state_folder(root)
                .map(|d| (d, false))
                .map_err(FolderError::Step)
        }
        Ok(None) => {}
        Err(e) => return Err(FolderError::Step(Step::Io(e))),
    }
    if crate::engines::in_c3_child() {
        return Err(FolderError::Child);
    }
    // `.eck` a real directory - or one that cannot be read (fail closed): only under the lock
    let _lock = match contain::entry(root.dir(), OsStr::new(LEGACY_STATE_DIR)) {
        Ok(Some(Kind::Dir)) | Err(_) => Some(
            MoveLock::acquire(repo_root, "create the project folder").map_err(FolderError::Held)?,
        ),
        Ok(_) => None,
    };
    // the state again (under the lock when there is one): the create says whether `.c3` is there now
    let fresh = match root.dir().create_dir(STATE_DIR) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(e) => return Err(FolderError::Step(Step::Create(e))),
    };
    let d = open_state_folder(root).map_err(FolderError::Step)?;
    if fresh {
        let _ = write_marker(&d);
    }
    Ok((d, fresh))
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
/// recovery records), or - an owner that cannot be shown alive - when it is older than
/// [`LOCK_STALE_SECS`] (never while its owner lives, R7b). Released (the file removed) on
/// drop. (F57-4) Its creation, its takeover and its release each happen under the
/// [`takeover_gate`]. The mover holds it from the re-read of the state through the rename; a run
/// that starts under the legacy folder holds it while it creates its records ([`begin_run`]); a
/// writer holds it while it creates a fresh `.c3/` beside a real `.eck/` ([`state_folder`]).
#[derive(Debug)]
pub struct MoveLock {
    path: PathBuf,
    token: String,
}

impl Drop for MoveLock {
    fn drop(&mut self) {
        // under the gate, so a takeover of this very record cannot fall between the compare and
        // the removal (without the gate the release still happens, as before)
        let _gate = takeover_gate(&self.path);
        if read_lock(&self.path).is_some_and(|r| r.token == self.token) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// The name of the gate file of the move lock at `lock`: `<lock>.gate`, next to it.
fn gate_path(lock: &Path) -> PathBuf {
    let mut name = lock.as_os_str().to_os_string();
    name.push(".gate");
    PathBuf::from(name)
}

/// (0.2.5, found by the acceptance review; F57-4) The gate of the move lock: an OS lock on the file
/// `<lock>.gate`, held for each step that changes who owns the lock - the CREATION (from before the
/// exclusive create through the complete, flushed record and its read-back, or the removal of a
/// file whose write failed: [`try_create_lock`]), the TAKEOVER's compare-and-remove
/// ([`take_over_if_stale`]) and the owner's RELEASE (drop). The takeover read the record, judged it
/// stale, read it again and removed the file - two separate steps: of two processes that judged the
/// same stale record, the first could remove it and create its own lock between the second's
/// compare and its removal, the second then removed THAT lock and created another, and both held
/// "the" lock - two movers in the section the lock exists to keep single. Under the gate the compare
/// and the removal are one step among C3 processes. (F57-4) The creation was outside it: a creator
/// suspended between its exclusive create and its write for longer than [`LOCK_GARBAGE_SECS`] had
/// its still-empty file taken over as garbage, and when it resumed its write into the unlinked
/// file succeeded (Unix) and it returned as a second owner. Now a creator's empty file exists only
/// while its creator holds the gate, a taker cannot remove it meanwhile, and the creator returns
/// success only after reading its own record back under the gate. The OS releases the lock when
/// its holder dies, so the gate has no stale state of its own (an empty file a creator left by
/// dying inside the gate is garbage after the interval, as before), and the gate file is never
/// removed (removing a locked file would bring the same race back). What it cannot do: it is an
/// OS lock, so it binds the C3 processes of one machine (and of hosts whose shared file system has
/// working OS locks); a process of ANOTHER host on a file system without working OS locks is held
/// only by the age rules ([`LOCK_STALE_SECS`], [`LOCK_GARBAGE_SECS`]) - the read-back still keeps a
/// creator whose file was replaced from returning as an owner - and a c3 older than 0.2.5 takes no
/// gate at all. `None` when the gate cannot be opened or is not free within a second: a taker then
/// removes nothing, a creator creates nothing.
fn takeover_gate(lock: &Path) -> Option<std::fs::File> {
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(gate_path(lock))
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match f.try_lock() {
            Ok(()) => return Some(f),
            Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(_) => return None,
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
                // (F57-4) the gate is busy (another process creates, takes over or releases the
                // lock now), or the file this creator wrote vanished under it: nothing was created
                // - wait, then try again or report the holder
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
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

/// Create the lock file by exclusive create and write this process's record into it. (0.2.5,
/// acceptance F57-4) All of it under the [`takeover_gate`]: the gate is taken BEFORE the exclusive
/// create and held through the complete, flushed record - or, when the write fails, through the
/// removal of the file - and through a read-back that must find exactly this process's record in
/// the file; only then is the lock this process's. `Err` of kind `WouldBlock`: the gate was not
/// free within its bounded wait (nothing was created), or the file vanished before the read-back -
/// the caller waits and tries again. `AlreadyExists`: a lock file is there - or the file read back
/// holds another record (left as it is: it is not this process's) - the caller judges it or
/// reports "held".
fn try_create_lock(path: &Path, purpose: &str) -> std::io::Result<MoveLock> {
    use std::io::Write as _;
    let Some(_gate) = takeover_gate(path) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "the gate of the move lock is held by another c3 process",
        ));
    };
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    // (the race test pauses a creator here, inside the gate)
    hold_at(LOCK_CREATE_HOLD_HOOK);
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
    drop(f);
    // still under the gate: the file at the lock's path holds this process's record - else the
    // lock is not this process's (a write into a file that was unlinked meanwhile never counts)
    match std::fs::read(path) {
        Ok(now) if now == bytes => Ok(MoveLock {
            path: path.to_path_buf(),
            token: rec.token,
        }),
        Ok(_) => Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "the move lock file holds another process's record",
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "the move lock file vanished while it was created",
        )),
        Err(e) => {
            let _ = std::fs::remove_file(path);
            Err(e)
        }
    }
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

/// Remove a stale lock so the caller can retry the exclusive create: one whose owner is gone, one
/// whose owner cannot be shown alive (another host's) and is older than [`LOCK_STALE_SECS`], or one
/// that does not parse and is older than [`LOCK_GARBAGE_SECS`]. (R7b, F48-2) The lock of an owner
/// that is alive on this machine is NEVER taken over, whatever its age: the caller waits its bounded
/// wait and goes on without moving. The final compare and the removal happen under the
/// [`takeover_gate`]. Returns whether the caller should retry at once (taken away, or already
/// gone).
fn take_over_if_stale(path: &Path) -> bool {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return true,
        Err(_) => return false,
    };
    if !lock_is_stale(&bytes, file_age_secs(path)) {
        return false;
    }
    // only the record that was judged is removed, and the compare and the removal are one step
    // among C3 processes ([`takeover_gate`]): another taker may have replaced it meanwhile
    let Some(_gate) = takeover_gate(path) else {
        return false;
    };
    match std::fs::read(path) {
        Ok(now) if now == bytes => std::fs::remove_file(path).is_ok(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
        _ => false,
    }
}

/// The stale rule of [`take_over_if_stale`] on a lock file's bytes and its age, judged only.
fn lock_is_stale(bytes: &[u8], age: Option<u64>) -> bool {
    match serde_json::from_slice::<LockRec>(bytes) {
        Ok(rec) if lock_is_here(&rec) => {
            // (a pid reused - even by this process - has another start time: gone)
            !crate::liveness::proc::pid_alive(rec.pid, &rec.start_time)
        }
        Ok(_) => age.is_some_and(|a| a > LOCK_STALE_SECS),
        Err(_) => age.is_none_or(|a| a > lock_garbage_secs()),
    }
}

/// Whether a lock record is this machine's (an empty host counts as this machine's).
fn lock_is_here(rec: &LockRec) -> bool {
    rec.host.is_empty()
        || rec
            .host
            .eq_ignore_ascii_case(&c3_core::host::machine_name())
}

/// Who holds the lock, in words ([`lock_words`]).
fn holder_words(path: &Path) -> String {
    match read_lock(path) {
        Some(r) => lock_words(&r),
        None => "the move lock is held by another c3 process".to_string(),
    }
}

/// (0.2.5 D5) A lock record in words the operator can find its process by: the owner's pid, its
/// host when it is another host's lock, what it holds the lock for and since when (the time the
/// record was written, in UTC to the second). The record's own text is shown with its control and
/// format characters replaced by `?`.
fn lock_words(r: &LockRec) -> String {
    let plain = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_control() || crate::unicode::is_format_char(c) {
                    '?'
                } else {
                    c
                }
            })
            .collect()
    };
    let host = if lock_is_here(r) {
        String::new()
    } else {
        format!(" on host {}", plain(&r.host))
    };
    let since = chrono::DateTime::parse_from_rfc3339(&r.created)
        .map(|t| {
            t.with_timezone(&chrono::Utc)
                .format("%Y-%m-%d %H:%M:%S UTC")
                .to_string()
        })
        .unwrap_or_else(|_| plain(&r.created));
    format!(
        "the move lock is held by c3 pid {}{host} ({}) since {since}",
        r.pid,
        plain(&r.purpose)
    )
}

/// (0.2.5 D5) Who holds the move lock of `repo_root` now, in [`lock_words`]' words; `None` when no
/// lock is there or the one there is stale (the next writing command takes it over). Read only -
/// `c3 doctor` names the holder in its line for the folder.
pub fn move_lock_holder(repo_root: &Path) -> Option<String> {
    let path = lock_path(repo_root);
    let bytes = std::fs::read(&path).ok()?;
    if lock_is_stale(&bytes, file_age_secs(&path)) {
        return None;
    }
    Some(match serde_json::from_slice::<LockRec>(&bytes) {
        Ok(r) => lock_words(&r),
        Err(_) => "the move lock is held by another c3 process".to_string(),
    })
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
    // the state again, under the lock and (R7b, F47-2) through the repository root's handle:
    // another process may have moved it meanwhile
    let root = match Root::open_anchor(repo_root) {
        Ok(r) => r,
        Err(e) => {
            out.deferred = Some(format!("the repository root {}", e.words()));
            return out;
        }
    };
    match contain::entry(root.dir(), OsStr::new(STATE_DIR)) {
        Ok(None) => {}
        Ok(Some(_)) => return out,
        Err(e) => {
            out.deferred = Some(format!("{STATE_DIR} cannot be read ({e})"));
            return out;
        }
    }
    let eck_dir = match contain::open_child(root.dir(), OsStr::new(LEGACY_STATE_DIR), false) {
        Ok(d) => d,
        Err(Step::Link { .. }) => {
            out.notes.push(legacy_link_note(&eck));
            return out;
        }
        Err(_) => return out,
    };
    if let Some(why) = live_run_under(repo_root, &eck) {
        out.deferred = Some(why);
        return out;
    }
    hold_inside_the_move();

    // The old tool's leftovers and report, relocated inside `.eck/` before the rename.
    let rel = relocate_leftovers(&eck_dir, LEGACY_STATE_DIR);
    // (Windows: a handle held on `.eck` would refuse its rename)
    drop(eck_dir);
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
        rename_folder(&root)
    };
    // (R7b, F48-3) the move is complete: the marker goes in last, through `.c3/`'s handle
    if renamed.is_ok() {
        if let Ok(d) = open_state_folder(&root) {
            let _ = write_marker(&d);
        }
    }
    drop(root);
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

/// (R7b, F47-2) The rename of `.eck` to `.c3`, relative to the repository root's handle and never
/// over an existing name: immediately before it `.c3` must be absent and `.eck` a real directory,
/// both read through the root's handle without following them; the rename moves the entry `.eck`
/// itself (a link put there in the last instant would be moved as a link - its target never). The
/// mover holds the move lock, and (0.2.5, acceptance answer 2) no C3 process creates `.c3` beside a
/// real `.eck/` without holding that lock ([`state_folder`]), so no C3 writer can make `.c3` in that
/// instant - whatever its migration switch. An entry made there by something else (the operator,
/// another tool) is the rest: a directory is never renamed over an existing name on Windows; on
/// Unix `rename(2)` could replace only an EMPTY directory made at `.c3` in that same instant (a
/// non-empty one, or any other entry, makes it fail, and the failure is read like any other rename
/// error).
fn rename_folder(root: &Root) -> std::io::Result<()> {
    if contain::entry(root.dir(), OsStr::new(STATE_DIR))?.is_some() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{STATE_DIR} appeared meanwhile"),
        ));
    }
    if contain::entry(root.dir(), OsStr::new(LEGACY_STATE_DIR))? != Some(Kind::Dir) {
        return Err(std::io::Error::other(format!(
            "{LEGACY_STATE_DIR} is no longer a real directory"
        )));
    }
    contain::race_point("state-rename");
    root.dir().rename(LEGACY_STATE_DIR, root.dir(), STATE_DIR)
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
    hold_at(MOVE_HOLD_HOOK);
}

/// A hold hook ([`MOVE_HOLD_HOOK`], [`LOCK_CREATE_HOLD_HOOK`]): with the hook set to `<value>`,
/// create `<value>.held` and wait (at most 60 s) until `<value>` exists.
fn hold_at(hook: &str) {
    let Some(v) = c3_core::test_hooks::hook(hook) else {
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
/// the folder in the notes. (R7b, F47-2) Through handles: `folder` is the folder's handle, `legacy/`
/// is opened (created) once relative to it without following it, and every step is a rename
/// between those handles - a directory swapped for a link after it was opened cannot take a
/// leftover outside.
fn relocate_leftovers(folder: &Dir, label: &str) -> Relocation {
    let mut out = Relocation::default();
    let present = |d: &Dir, n: &str| contain::entry(d, OsStr::new(n)).ok().flatten().is_some();
    let mut legacy: Option<Result<Dir, String>> = None;
    for name in LEGACY_LEFTOVERS {
        if !present(folder, name) {
            continue;
        }
        let ready = legacy.get_or_insert_with(|| open_legacy(folder, label));
        let result = match ready {
            Err(why) => Err(why.clone()),
            Ok(dir) => {
                let mut dst = name.to_string();
                let mut n = 1;
                while present(dir, &dst) {
                    dst = format!("{name}.{n}");
                    n += 1;
                }
                folder
                    .rename(name, dir, &dst)
                    .map_err(|e| format!("could not move it to {label}/{LEGACY_SUBDIR}/ ({e})"))
            }
        };
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
    let snap_link = |why: &Step| {
        format!(
            "left {label}/{}: it is {} - nothing is taken through it",
            LEGACY_REPORT[0],
            step_link_words(why)
        )
    };
    match contain::entry(folder, OsStr::new(LEGACY_REPORT[0])) {
        Ok(Some(Kind::Link { symlink, .. })) => out.left.push(snap_link(&Step::Link { symlink })),
        Ok(Some(Kind::Dir)) => {
            match contain::open_child(folder, OsStr::new(LEGACY_REPORT[0]), false) {
                Ok(snap) => {
                    contain::race_point("state-report");
                    let report = OsStr::new(LEGACY_REPORT[1]);
                    if contain::entry(&snap, report).ok().flatten() == Some(Kind::File) {
                        if present(folder, LAST_REPORT) {
                            out.left.push(format!(
                                "left {label}/{}/{}: {label}/{LAST_REPORT} exists already",
                                LEGACY_REPORT[0], LEGACY_REPORT[1]
                            ));
                        } else {
                            match snap.rename(report, folder, LAST_REPORT) {
                            Ok(()) => out.report = true,
                            Err(e) => out.left.push(format!(
                                "left {label}/{}/{}: could not move it to {label}/{LAST_REPORT} ({e})",
                                LEGACY_REPORT[0], LEGACY_REPORT[1]
                            )),
                        }
                        }
                    }
                }
                Err(why @ Step::Link { .. }) => out.left.push(snap_link(&why)),
                Err(_) => {}
            }
        }
        _ => {}
    }
    out
}

/// The old tool's `legacy/` in `folder`, opened (created when missing) relative to the folder's
/// handle without following it; `Err` says why nothing may be moved into it.
fn open_legacy(folder: &Dir, label: &str) -> Result<Dir, String> {
    let d = match contain::open_child(folder, OsStr::new(LEGACY_SUBDIR), true) {
        Ok(d) => d,
        Err(why @ Step::Link { .. }) => {
            return Err(format!(
                "{label}/{LEGACY_SUBDIR} is {} - never written through",
                step_link_words(&why)
            ))
        }
        Err(Step::NotDir) => {
            return Err(format!(
                "{label}/{LEGACY_SUBDIR} is not a plain directory - never written through"
            ))
        }
        Err(Step::Create(e)) => {
            return Err(format!("could not create {label}/{LEGACY_SUBDIR}/ ({e})"))
        }
        Err(why) => return Err(format!("{label}/{LEGACY_SUBDIR} {why}")),
    };
    // (the race tests swap `legacy/` for a link here, between its check and the relocation)
    contain::race_point("state-relocate");
    Ok(d)
}

/// (S2, F36-2; R7b, F48-3) Finish a `.c3/` that is not marked finished ([`LAYOUT_MARKER`]): an S1
/// build's half-done move, a move killed before its marker, a folder an earlier build made. Under
/// the move lock, the old tool's `anchor`, `update_seq` and `telemetry_queue.json` at its top - a
/// lone `anchor` too - go to `.c3/legacy/` and `lastsnapshot/AnswerToSA.md` becomes
/// `last-report.md`; then the marker is written. A leftover that cannot be relocated leaves the
/// folder unmarked (the next writing command tries again). Nothing happens for a marked folder, so
/// C3's own `anchor` - written into a marked folder, or moved in by the cache move after this - is
/// never sent to `legacy/`.
fn finish_half_done(repo_root: &Path) -> Vec<String> {
    let c3 = path(repo_root);
    if !real_dir(&c3) || is_finished(&c3) {
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
    // (R7b, F47-2) under the lock, through `.c3/`'s handle (never followed)
    let Some(folder) = Root::open_anchor(repo_root)
        .ok()
        .and_then(|r| contain::open_child(r.dir(), OsStr::new(STATE_DIR), false).ok())
    else {
        return Vec::new();
    };
    if contain::entry(&folder, OsStr::new(LAYOUT_MARKER))
        .ok()
        .flatten()
        .is_some()
    {
        return Vec::new();
    }
    let rel = relocate_leftovers(&folder, STATE_DIR);
    let mut v: Vec<String> = rel
        .relocated_notes(STATE_DIR)
        .into_iter()
        .map(|n| format!("finished the move an earlier C3 left half done: {n}"))
        .collect();
    v.extend(rel.left);
    if rel.blocked.is_none() {
        if let Err(e) = write_marker(&folder) {
            v.push(format!(
                "{STATE_DIR}/{LAYOUT_MARKER} could not be written ({e}); the next writing command finishes the folder"
            ));
        }
    }
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
                // (R7b, F48-5) by design: the link blocks the move until it is gone; it is named
                return may(format!(
                    "{} is {} and the runs behind it are not checked (remove that link, or put a real directory in its place, and the move goes on)",
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
///
/// (R7b, F48-1) First, in every case: a collab directory that reaches into C3's folder through a
/// link ([`folder_link_problem`]: `.c3` or `.eck` itself a link, or an alias) refuses the run with
/// one line and nothing is created. A collab directory inside `.c3/` is then created through
/// handles ([`prepare_dir`]: never through a link, `.c3/` itself fresh with its marker - never inside
/// a C3 child), so the run's own directories are made in a real directory of the folder.
///
/// (0.2.5, acceptance F57-6) Then the run's own directories: the task directory `<collab>/<task>`
/// and each of `dirs` below it (`handoffs` for a consultation, a detached run and a panel; `work`
/// for a work run) are made ready by [`run_dirs`] - inside C3's folder through handles, a link at
/// either refusing the run before anything is written (under the move lock for a run under
/// `.eck/`). The callers create none of these directories themselves.
pub fn begin_run(
    repo_root: &Path,
    collab_root: &Path,
    task: &str,
    dirs: &[&str],
) -> Result<RunStart, String> {
    if let Some(why) = folder_link_problem(repo_root, collab_root) {
        return Err(format!("{why}; nothing was started"));
    }
    let start = |lock: Option<MoveLock>| -> Result<RunStart, String> {
        run_dirs(repo_root, collab_root, task, dirs)
            .map_err(|why| format!("{why}; nothing was started"))?;
        Ok(RunStart { _lock: lock })
    };
    if crate::engines::in_c3_child() {
        return start(None);
    }
    match split_state_path(repo_root, collab_root) {
        Some((StateName::Legacy, _)) => {}
        Some((StateName::Current, _)) | None => return start(None),
    }
    let eck = legacy_path(repo_root);
    if !real_dir(&eck) && !real_dir(&path(repo_root)) {
        // no folder at all: nothing a mover could take away (and C3 never creates `.eck/` again:
        // [`run_dirs`] refuses)
        return start(None);
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
    start(Some(lock))
}

/// (0.2.5, acceptance F57-6) The task directory `<collab_root>/<task>` and each of `dirs` below it
/// made ready for a run's records. Inside C3's project folder (the collab root lies in `.c3/` or in
/// the legacy `.eck/`) through handles from the repository root: the collab root as [`prepare_dir`]
/// makes it, then the task directory relative to the collab root's handle and each of `dirs`
/// relative to the task directory's, created when missing and opened WITHOUT following them - a
/// link (a symlink, a junction or another reparse point) or anything else that is not a real
/// directory at either refuses, in one line that names it, before anything is written there (its
/// target is untouched). Before this, the callers appended `<task>/handoffs` to a checked collab
/// root and created it with `create_dir_all`: a task directory that already was a link sent the
/// handoff, the reply, the events, the task lock and the ledger wherever it pointed.
///
/// A collab root OUTSIDE the folder (the default `.collab/`, a `--collab-dir` elsewhere) is the
/// operator's own anchor and stays as it was: the directories are created by path and a link in
/// it is followed. [`begin_run`] calls this for a single run, a detached run, a panel and a work
/// run; a panel member, whose panel made these directories under the move lock, calls it itself.
/// What it does not do: the run's later writes go by path into the directories checked here, so a
/// component swapped for a link AFTER this check, while the run writes, is not caught.
pub fn run_dirs(
    repo_root: &Path,
    collab_root: &Path,
    task: &str,
    dirs: &[&str],
) -> Result<(), String> {
    let plain = |n: &str| contain::components(Path::new(n)).is_some_and(|c| c.len() == 1);
    if let Some(bad) = std::iter::once(task)
        .chain(dirs.iter().copied())
        .find(|n| !plain(n))
    {
        return Err(format!("'{bad}' is not one plain directory name"));
    }
    let task_path = collab_root.join(task);
    if split_state_path(repo_root, collab_root).is_none() {
        let leaves: Vec<PathBuf> = if dirs.is_empty() {
            vec![task_path]
        } else {
            dirs.iter().map(|d| task_path.join(d)).collect()
        };
        for p in leaves {
            std::fs::create_dir_all(&p)
                .map_err(|e| format!("could not create {} ({e})", p.display()))?;
        }
        return Ok(());
    }
    let (collab, mut cur_path) = open_write_dir(repo_root, collab_root)?;
    cur_path.push(task);
    let task_dir = run_dir(repo_root, &collab, task, &cur_path)?;
    for d in dirs {
        run_dir(repo_root, &task_dir, d, &cur_path.join(d))?;
    }
    Ok(())
}

/// One directory of [`run_dirs`]: `name` in `parent`, created when missing, opened without following
/// it; `p` is its path for the refusal.
fn run_dir(repo_root: &Path, parent: &Dir, name: &str, p: &Path) -> Result<Dir, String> {
    match contain::open_child(parent, OsStr::new(name), true) {
        Ok(d) => Ok(d),
        Err(Step::Link { .. }) => Err(format!(
            "{} is {} - C3 never writes a run's records through a link inside its project folder (its target is untouched)",
            shown(repo_root, p),
            link_words(p)
        )),
        Err(why) => Err(format!("{} {why}", shown(repo_root, p))),
    }
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

/// `p` with `.` dropped and `..` taking the previous component away (never above the root), and
/// (R7b, F47-3) each name as the file system resolves it ([`fs_name`]).
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
            Component::Normal(n) => out.push(fs_name(n)),
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// (R7b, F47-3) A path component as the file system resolves it: on Windows trailing dots and
/// spaces name the same entry (`.eck.`, `.c3 ` are `.eck` and `.c3`), so they are dropped before a
/// name is compared or joined; a component that is nothing but dots and spaces is kept as it is.
/// Elsewhere such a name is another entry and stays exact.
fn fs_name(n: &std::ffi::OsStr) -> std::ffi::OsString {
    if cfg!(windows) {
        if let Some(s) = n.to_str() {
            let t = s.trim_end_matches(['.', ' ']);
            if !t.is_empty() {
                return t.into();
            }
        }
    }
    n.to_os_string()
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
///
/// (R7b, F48-4) A path whose text names neither folder is classified by IDENTITY too: the longest
/// existing prefix of it in the repository is compared with the existing `.c3` and `.eck` (their
/// canonical paths, and the directory identity of an open handle), so an 8.3 short name (`ECK~1`)
/// and an alias that is a link to the folder are the folder (an alias is then refused by every
/// writer, [`folder_link_problem`]).
fn split_state_path(repo_root: &Path, p: &Path) -> Option<(StateName, String)> {
    let rel = repo_relative_normal(repo_root, p)?;
    let parts: Vec<&str> = rel
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    let first = parts.first()?;
    let kind = if names_equal(first, STATE_DIR) {
        StateName::Current
    } else if names_equal(first, LEGACY_STATE_DIR) {
        StateName::Legacy
    } else {
        return split_by_identity(repo_root, &parts);
    };
    Some((kind, parts[1..].join("/")))
}

/// The identity half of [`split_state_path`]: the first existing prefix of `parts` (below the
/// repository root) that is the same directory as an existing `.c3` or `.eck`.
fn split_by_identity(repo_root: &Path, parts: &[&str]) -> Option<(StateName, String)> {
    let id = |p: &Path| Root::open_anchor(p).ok().map(|r| r.id());
    let canon = |p: &Path| {
        std::fs::canonicalize(p)
            .ok()
            .map(crate::providers::dunce_simplify)
    };
    let folders: Vec<(StateName, Option<PathBuf>, Option<contain::FileId>)> = [
        (StateName::Current, STATE_DIR),
        (StateName::Legacy, LEGACY_STATE_DIR),
    ]
    .into_iter()
    .filter_map(|(kind, name)| {
        let f = repo_root.join(name);
        std::fs::symlink_metadata(&f).ok()?;
        Some((kind, canon(&f), id(&f)))
    })
    .collect();
    if folders.is_empty() {
        return None;
    }
    let mut cur = repo_root.to_path_buf();
    for (i, part) in parts.iter().enumerate() {
        cur.push(part);
        if std::fs::symlink_metadata(&cur).is_err() {
            return None;
        }
        let (c, d) = (canon(&cur), id(&cur));
        for (kind, fc, fd) in &folders {
            let same = (c.is_some() && c == *fc) || (d.is_some() && d == *fd);
            if same {
                return Some((*kind, parts[i + 1..].join("/")));
            }
        }
    }
    None
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
/// (R7b, F47-2) The walk and every creation go through handles from the repository root
/// ([`open_write_dir`]); a caller that then writes by path into the directory it got does so by
/// name (see the module doc for which do).
pub fn prepare_dir(repo_root: &Path, dir: &Path) -> Result<PathBuf, String> {
    open_write_dir(repo_root, dir).map(|(_, p)| p)
}

/// (R7b, F47-2) [`prepare_dir`] on handles: the directory's handle and its path. The repository
/// root is opened once as an anchor, the folder relative to it WITHOUT following it (a link or a
/// non-directory refuses; `.c3` is created when missing, `.eck` never), and each directory below
/// relative to its parent's handle the same way, created when missing.
fn open_write_dir(repo_root: &Path, dir: &Path) -> Result<(Dir, PathBuf), String> {
    for attempt in 0..2 {
        let p = moved_with_folder(repo_root, dir);
        let Some((kind, rest)) = split_state_path(repo_root, &p) else {
            std::fs::create_dir_all(&p).map_err(|e| format!("create {}: {e}", p.display()))?;
            let d = Dir::open_ambient_dir(&p, cap_std::ambient_authority())
                .map_err(|e| format!("create {}: {e}", p.display()))?;
            return Ok((d, p));
        };
        let root = Root::open_anchor(repo_root)
            .map_err(|e| format!("the repository root {} - nothing was written", e.words()))?;
        let name = match kind {
            StateName::Current => STATE_DIR,
            StateName::Legacy => LEGACY_STATE_DIR,
        };
        let mut cur_path = repo_root.join(name);
        // `.c3` is created when missing (fresh, with its marker; beside a real `.eck/` only under
        // the move lock - [`state_folder`]); `.eck` never is
        let opened = match kind {
            StateName::Current => match state_folder(repo_root, &root) {
                Ok((d, _)) => Ok(d),
                Err(FolderError::Step(s)) => Err(s),
                Err(FolderError::Held(why)) => {
                    return Err(format!(
                        "{STATE_DIR}/ could not be created now ({why}) - nothing was written"
                    ))
                }
                Err(FolderError::Child) => return Err(CHILD_REFUSAL.to_string()),
            },
            StateName::Legacy => contain::open_child(root.dir(), OsStr::new(name), false),
        };
        let mut cur = match opened {
            Ok(d) => d,
            Err(Step::Link { .. }) => return Err(link_refusal(repo_root, &cur_path)),
            Err(Step::NotDir) => {
                return Err(format!(
                    "{} is not a directory - nothing was written",
                    shown(repo_root, &cur_path)
                ))
            }
            Err(Step::Missing) if attempt == 0 => continue,
            Err(Step::Missing) => {
                return Err(format!(
                    "the project folder {LEGACY_STATE_DIR}/ is gone and {STATE_DIR}/ is not there - nothing was written (C3 never creates {LEGACY_STATE_DIR}/ again)"
                ))
            }
            Err(Step::Create(e)) => return Err(format!("create {STATE_DIR}/: {e}")),
            Err(why) => {
                return Err(format!(
                    "{} {why} - nothing was written",
                    shown(repo_root, &cur_path)
                ))
            }
        };
        let mut vanished = false;
        for part in rest.split('/').filter(|s| !s.is_empty()) {
            cur_path.push(part);
            cur = match contain::open_child(&cur, OsStr::new(part), true) {
                Ok(d) => d,
                Err(Step::Link { .. }) => return Err(link_refusal(repo_root, &cur_path)),
                Err(Step::NotDir) => {
                    return Err(format!(
                        "{} is not a directory - nothing was written",
                        shown(repo_root, &cur_path)
                    ))
                }
                // the folder was removed under this writer: resolve it again
                Err(Step::Create(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                    vanished = true;
                    break;
                }
                Err(Step::Missing) => {
                    vanished = true;
                    break;
                }
                Err(Step::Create(e)) => {
                    return Err(format!("create {}: {e}", shown(repo_root, &cur_path)))
                }
                Err(why) => return Err(format!("create {}: {why}", shown(repo_root, &cur_path))),
            };
        }
        if !vanished {
            return Ok((cur, cur_path));
        }
    }
    Err(format!(
        "{} could not be prepared: the project folder moved while it was written - nothing was written",
        shown(repo_root, dir)
    ))
}

/// (R7b, F47-2) What a file held before [`write_in`] replaces it, read through the same directory
/// handle: `Ok(None)` when it was not there, `Err` when it could not be read as text.
pub type Current = Result<Option<String>, String>;

/// (S2, F36-1/F36-3) Write the file `name` in the folder directory `dir` (prepared as by
/// [`prepare_dir`]): its bytes come from `content`, called with the file's path and (R7b) what the
/// file holds now, read through the directory's handle (so a read-modify-write reads the file it
/// writes). A file that is a link, or no regular file, is refused. (R7b, F47-2) Through handles:
/// the directory is reached by [`open_write_dir`] and the file published by a new temporary file
/// and a rename relative to that handle ([`contain::publish`]) - it REPLACES the file (a second
/// hard link of it never sees the write) and a directory swapped for a link meanwhile cannot take
/// it outside. When the folder was removed between the preparation and the write, the write is
/// made again at the folder's place now. Returns the file written. (0.2.5 D5) Before it writes, it
/// sweeps the directory of its own stale temporary files ([`is_write_temp_name`], unchanged for
/// [`WRITE_TEMP_STALE_SECS`] or more - a writer killed between its write and its rename left them)
/// through the same handle ([`contain::sweep_stale_temps`]: regular files only, no link followed,
/// every failure ignored).
pub fn write_in(
    repo_root: &Path,
    dir: &Path,
    name: &str,
    content: impl Fn(&Path, Current) -> Result<Vec<u8>, String>,
) -> Result<PathBuf, String> {
    let mut last = String::new();
    let leaf = OsStr::new(name);
    for _ in 0..2 {
        let (d, dpath) = open_write_dir(repo_root, dir)?;
        let f = dpath.join(name);
        let check = |_: contain::Stage| -> Result<(), String> {
            match contain::entry(&d, leaf) {
                Ok(Some(Kind::Link { .. })) => Err(link_refusal(repo_root, &f)),
                Ok(Some(Kind::File)) | Ok(None) => Ok(()),
                Ok(Some(_)) => Err(format!(
                    "write {}: it is not a regular file",
                    shown(repo_root, &f)
                )),
                Err(e) => Err(format!("write {}: {e}", shown(repo_root, &f))),
            }
        };
        check(contain::Stage::BeforeCreate)?;
        contain::sweep_stale_temps(&d, is_write_temp_name, WRITE_TEMP_STALE_SECS);
        let current: Current = match contain::read_file_in(&d, leaf) {
            Ok(b) => String::from_utf8(b)
                .map(Some)
                .map_err(|_| "stream did not contain valid UTF-8".to_string()),
            Err(Step::Missing) => Ok(None),
            Err(why) => Err(format!("it {why}")),
        };
        let bytes = content(&f, current)?;
        #[cfg(unix)]
        let permissions = d
            .symlink_metadata(leaf)
            .ok()
            .filter(|m| m.is_file())
            .map(|m| m.permissions());
        #[cfg(not(unix))]
        let permissions = None;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|t| t.as_nanos())
            .unwrap_or(0);
        let tmp = write_temp_name(name, std::process::id(), nanos);
        // (the race tests swap the directory for a link at `state-write-create`)
        let published = contain::publish(&contain::Publish {
            dir: &d,
            leaf,
            tmp: OsStr::new(&tmp),
            bytes: &bytes,
            permissions,
            check: &check,
            race: "state-write",
        });
        let failure = match published {
            Ok(()) => return Ok(f),
            Err(failure) => failure,
        };
        let msg = match &failure.error {
            contain::PublishError::Check(why) => why.clone(),
            // the folder was removed under this writer: resolve it again
            contain::PublishError::Create(e) if e.kind() == std::io::ErrorKind::NotFound => {
                last = format!("write {}: {e}", shown(repo_root, &f));
                continue;
            }
            contain::PublishError::Create(e)
            | contain::PublishError::Write(e)
            | contain::PublishError::Rename(e) => format!("write {}: {e}", shown(repo_root, &f)),
        };
        return Err(failure.with_cleanup(msg));
    }
    Err(last)
}

/// (0.2.5 D5) From this age (seconds since its last change) a temporary file of [`write_in`]'s own
/// pattern ([`is_write_temp_name`]) is the debris of a writer killed between its write and its
/// rename: the next write into that directory removes it.
pub const WRITE_TEMP_STALE_SECS: u64 = 24 * 60 * 60;

/// The temporary name [`write_in`] writes `name` under before its rename:
/// `.<name>.c3-<pid>-<nanos>.tmp`.
fn write_temp_name(name: &str, pid: u32, nanos: u128) -> String {
    format!(".{name}.c3-{pid}-{nanos}.tmp")
}

/// (0.2.5 D5) Whether `name` is a temporary name of [`write_in`]'s own pattern
/// ([`write_temp_name`]): a dot, a non-empty file name, `.c3-`, the writer's pid and a time in
/// nanoseconds (ASCII digits both), `.tmp` - nothing else.
fn is_write_temp_name(name: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let Some(rest) = name.strip_prefix('.').and_then(|r| r.strip_suffix(".tmp")) else {
        return false;
    };
    let Some((file, ids)) = rest.rsplit_once(".c3-") else {
        return false;
    };
    match ids.split_once('-') {
        Some((pid, nanos)) => !file.is_empty() && digits(pid) && digits(nanos),
        None => false,
    }
}

// ------------------------------------------------------------------------------ the collab cache

/// (R7b, F47-2) The legacy cache `<collab_root>/.c3` opened through handles, with its parent's
/// handle. When it lies in the repository: from the repository root's handle one component at a
/// time, none followed; outside it: the collab root opened without following it, then its `.c3`.
/// `Err(Some(p))`: the link at `p` stopped it; `Err(None)`: it is not there (or no directory).
fn open_cache_source(repo_root: &Path, collab_root: &Path) -> Result<(Dir, Dir), Option<PathBuf>> {
    let src = collab_root.join(STATE_DIR);
    let Some(rel) = repo_relative_normal(repo_root, &src) else {
        let parent = match Root::open(collab_root) {
            Ok(r) => r.dir().try_clone().map_err(|_| None)?,
            Err(RootError::Link) => return Err(Some(collab_root.to_path_buf())),
            Err(_) => return Err(None),
        };
        return match contain::open_child(&parent, OsStr::new(STATE_DIR), false) {
            Ok(d) => Ok((parent, d)),
            Err(Step::Link { .. }) => Err(Some(src)),
            Err(_) => Err(None),
        };
    };
    let root = Root::open_anchor(repo_root).map_err(|_| None)?;
    let mut cur = root.dir().try_clone().map_err(|_| None)?;
    let mut cur_path = repo_root.to_path_buf();
    let mut parent = None;
    for part in rel.split('/').filter(|s| !s.is_empty()) {
        cur_path.push(part);
        let next = match contain::open_child(&cur, OsStr::new(part), false) {
            Ok(d) => d,
            Err(Step::Link { .. }) => return Err(Some(cur_path)),
            Err(_) => return Err(None),
        };
        parent = Some(std::mem::replace(&mut cur, next));
    }
    Ok((parent.ok_or(None)?, cur))
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
    if std::fs::symlink_metadata(&src).is_err() || same_dir(&src, &dst) {
        return notes;
    }
    let src_label = shown(repo_root, &src);
    // (R7b, F47-2) through handles: the source and `.c3/` are opened without following them, and
    // each entry is renamed between the two handles
    let (parent, src_dir) = match open_cache_source(repo_root, collab_root) {
        Ok(v) => v,
        Err(Some(link)) => {
            notes.push(format!(
                "{} is {} - the cache under it is not moved into {STATE_DIR}/ (C3 never walks through a link)",
                shown(repo_root, &link),
                link_words(&link)
            ));
            return notes;
        }
        Err(None) => return notes,
    };
    let entries = match contain::list(&src_dir) {
        Ok(v) => v,
        Err(_) => return notes,
    };
    if entries.is_empty() {
        drop(src_dir);
        let _ = parent.remove_dir(STATE_DIR);
        return notes;
    }
    let dst_dir = match Root::open_anchor(repo_root)
        .map_err(|e| format!("the repository root {}", e.words()))
        .and_then(|r| {
            state_folder(repo_root, &r)
                .map(|(d, _)| d)
                .map_err(|why| match why {
                    FolderError::Step(Step::Create(e)) => {
                        format!("could not create {STATE_DIR}/ ({e})")
                    }
                    FolderError::Step(_) => format!("{STATE_DIR} is a link or not a directory"),
                    FolderError::Held(why) => {
                        format!("{STATE_DIR}/ could not be created now ({why})")
                    }
                    FolderError::Child => CHILD_REFUSAL.to_string(),
                })
        }) {
        Ok(d) => d,
        Err(why) => {
            notes.push(format!("{src_label} not moved: {why}"));
            return notes;
        }
    };
    let mut moved: Vec<String> = Vec::new();
    for name in entries {
        let n = name.to_string_lossy().to_string();
        if contain::entry(&dst_dir, &name).ok().flatten().is_some() {
            notes.push(format!(
                "left {src_label}/{n}: {STATE_DIR}/{n} exists already (the destination wins)"
            ));
            continue;
        }
        contain::race_point("state-cache");
        match src_dir.rename(&name, &dst_dir, &name) {
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
    drop(src_dir);
    let _ = parent.remove_dir(STATE_DIR);
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
