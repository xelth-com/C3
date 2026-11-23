//! C3's per-project folder (0.2.5, wave S1): `<repo>/.c3/`, lowercase, at the repository root,
//! git-ignored as a whole. It holds the manifests (`ROADMAP.md`, `CONTEXT.md`, ... and
//! `profiles.json`), `last-report.md` (the report of the last `c3 finish` / `c3 fail`), `scouts/`,
//! the hand-off `snapshots/` and `lastsnapshot/` (written only on demand), and C3's own cache -
//! `anchor`, `snapshot_seq`, `index/`, `explains/` - one per repository.
//!
//! The folder was inherited as `.eck/` from another tool. This module is the ONE place that names
//! both folders ([`STATE_DIR`], [`LEGACY_STATE_DIR`]) and the ignore files ([`IGNORE_FILE`], with
//! [`LEGACY_IGNORE_FILE`] as the fallback), and it holds the two operations on the folder:
//!
//! * [`ensure`] - called by every command that WRITES into the folder: it moves an existing `.eck/`
//!   once ([`migrate_legacy`]) and a legacy cache `<collab root>/.c3/` entry by entry
//!   ([`migrate_collab_cache`]), creates `.c3/`, writes each missing stub manifest and adds `.c3/`
//!   to `.gitignore` when no line ignores it. Inside a C3 child (`C3_CHILD`) it creates and moves
//!   nothing: a folder that is not there is a refusal.
//! * [`read_path`] - what the read-only commands read: `.c3/`, else `.eck/` until a writing command
//!   moved it.
//! * [`collab_dir`] - (point 11) a collab directory kept inside the folder (`.eck/collab`) follows
//!   the move; every collab path a command resolves goes through it.
//!
//! The move never runs through a link (a `.eck` that is a symlink, a junction or another reparse
//! point is left alone), never over an existing path (`.c3` in any form stops it), never while a
//! run is live under `.eck/` ([`live_run_under`]), and is one `rename` or nothing (when it cannot
//! be made now, the command goes on with the legacy folder for that run and says so). It is off
//! with `C3_STATE_MIGRATE=off` or `"state_migrate": false` in the C3 config. The SessionStart hook
//! never moves anything ([`forbid_moves`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::setup::init::{ensure_gitignore_line, stub_content, STUB_MANIFESTS};

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
/// The switch that keeps C3's hands off `.eck/` (`off`).
pub const MIGRATE_ENV: &str = "C3_STATE_MIGRATE";

/// The old tool's files at the top of its folder that C3 never reads: moved into
/// [`LEGACY_SUBDIR`] (`anchor` in particular must not be taken for C3's own anchor).
const LEGACY_LEFTOVERS: &[&str] = &["anchor", "update_seq", "telemetry_queue.json"];
/// The old tool's report, relative to the folder.
const LEGACY_REPORT: [&str; 2] = ["lastsnapshot", "AnswerToSA.md"];

/// How a refusal inside a C3 child reads.
pub const CHILD_REFUSAL: &str = "C3 does not create or move its .c3/ folder inside a C3 child (C3_CHILD is set) - run the command outside the reviewer or worker";

/// `<repo>/.c3`.
pub fn path(repo_root: &Path) -> PathBuf {
    repo_root.join(STATE_DIR)
}

/// `<repo>/.eck`.
pub fn legacy_path(repo_root: &Path) -> PathBuf {
    repo_root.join(LEGACY_STATE_DIR)
}

/// The folder a read-only command reads: `.c3` when it is a directory, else `.eck` when that is
/// one (until a writing command moved it), else `None`.
pub fn read_path(repo_root: &Path) -> Option<PathBuf> {
    let c3 = path(repo_root);
    if c3.is_dir() {
        return Some(c3);
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

/// Whether C3 may move `.eck/` (default ON): `C3_STATE_MIGRATE=off` (the environment wins) or
/// `"state_migrate": false` in the C3 config keeps it untouched - then a fresh `.c3/` is created.
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
    /// move had to wait ([`Ensured::deferred`]).
    pub dir: PathBuf,
    /// One line per move ([`migrate_legacy`], [`migrate_collab_cache`]).
    pub moved: Vec<String>,
    /// The stub manifests written (repo-relative, `.c3/ROADMAP.md`).
    pub created: Vec<String>,
    /// The folder did not exist before this call.
    pub new_folder: bool,
    /// The `.c3/` line was added to `.gitignore`.
    pub gitignore_added: bool,
    /// (S1 point 11) Why the move of `.eck/` could not be made now (a run is live under it, or the
    /// rename failed): the command goes on with the legacy folder, nothing else was created.
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
/// ([`migrate_legacy`]), create the folder, move a legacy cache `<collab_root>/.c3/` into it
/// ([`migrate_collab_cache`], when the command knows its collab root), write each missing stub
/// manifest and add `.c3/` to `.gitignore` when no line ignores it. Inside a C3 child nothing is
/// created or moved: an existing folder is used as it is, a missing one is a refusal
/// ([`CHILD_REFUSAL`]). (S1 point 11) When the move cannot be made now (a run is live under
/// `.eck/`, or the rename failed) nothing is created: [`Ensured::dir`] is `.eck` for this run and
/// [`Ensured::deferred`] says why - the caller writes into `Ensured::dir`, never into `.c3/` blindly.
pub fn ensure(repo_root: &Path, collab_root: Option<&Path>) -> Result<Ensured, String> {
    let dir = path(repo_root);
    if crate::engines::in_c3_child() {
        return if dir.is_dir() {
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
    // a collab root inside the legacy folder moved with it
    let collab_root = collab_root.map(|c| moved_with_folder(repo_root, c));
    let collab_root = collab_root.as_deref();
    match std::fs::symlink_metadata(&dir) {
        Err(_) => {
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("could not create {STATE_DIR}/ ({e})"))?;
            out.new_folder = true;
        }
        Ok(_) if dir.is_dir() => {}
        Ok(_) => {
            return Err(format!(
                "{STATE_DIR} exists in the repository root and is not a directory - C3 keeps its project folder there; move it away"
            ))
        }
    }
    if let Some(c) = collab_root {
        out.moved.extend(migrate_collab_cache(repo_root, c));
    }
    for name in STUB_MANIFESTS {
        let file = format!("{name}.md");
        let p = dir.join(&file);
        if std::fs::symlink_metadata(&p).is_ok() {
            continue;
        }
        std::fs::write(&p, stub_content(name))
            .map_err(|e| format!("write {STATE_DIR}/{file}: {e}"))?;
        out.created.push(format!("{STATE_DIR}/{file}"));
    }
    out.gitignore_added = ensure_gitignore(repo_root)?;
    Ok(out)
}

/// The moves of `c3 doctor --fix`: [`migrate_legacy`] and [`migrate_collab_cache`], without the
/// stubs (`c3 doctor` reports, it does not provision). The caller keeps it out of a C3 child.
/// `Err("not moved: <why>")` when the move of `.eck/` had to wait (a run is live under it, or the
/// rename failed); nothing was moved then.
pub fn migrate(repo_root: &Path, collab_root: &Path) -> Result<Vec<String>, String> {
    let m = migrate_legacy(repo_root);
    if let Some(why) = m.deferred {
        return Err(format!("not moved: {why}"));
    }
    let mut v = m.notes;
    v.extend(migrate_collab_cache(
        repo_root,
        &moved_with_folder(repo_root, collab_root),
    ));
    Ok(v)
}

/// A path inside `<repo>/.eck/` that the folder's move has carried to `<repo>/.c3/`: the same
/// relative path under `.c3/` when `.eck` is gone and `.c3` is there; otherwise the path as given.
pub fn moved_with_folder(repo_root: &Path, p: &Path) -> PathBuf {
    match split_state_path(repo_root, p) {
        Some((StateName::Legacy, rest))
            if std::fs::symlink_metadata(legacy_path(repo_root)).is_err()
                && path(repo_root).is_dir() =>
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

/// What [`migrate_legacy`] did.
#[derive(Debug, Clone, Default)]
pub struct Migration {
    /// `.eck/` was renamed to `.c3/`.
    pub moved: bool,
    /// One line per thing done or left.
    pub notes: Vec<String>,
    /// (S1 point 11) The move was due but could not be made now - a run is live under `.eck/`, or
    /// the rename failed: why. `.eck/` is whole; the caller goes on with it for this run.
    pub deferred: Option<String>,
}

/// The test hook that makes the folder's rename fail (honoured only in test mode), so a failed
/// move can be tested without holding a file open.
pub const RENAME_FAIL_HOOK: &str = "CODEX_CONSULT_TEST_STATE_RENAME_FAIL";

/// Move an existing `<repo>/.eck` to `<repo>/.c3` - once: only when `.c3` does not exist in any
/// form, `.eck` is a real directory (a link is refused and left untouched), and the switch allows
/// it ([`migration_enabled`]). (S1 point 11) Not while a detached consultation, a work run or an
/// interrupted consultation's recovery record of the repository is live under `.eck/`
/// ([`live_run_under`]), and one `rename` or nothing: when it fails nothing else is tried - both
/// are reported as [`Migration::deferred`] ("not now"), never as a half-moved folder. After the
/// move: the old tool's leftovers at the top of the folder go to `.c3/legacy/`, its report
/// `lastsnapshot/AnswerToSA.md` becomes `last-report.md`, and `.c3/` is added to `.gitignore` (the
/// `.eck/` line is left alone). The caller keeps it out of the hook and of a C3 child.
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
    if crate::setup::repo_models::is_link(&meta) {
        out.notes.push(format!(
            "{LEGACY_STATE_DIR} is {} - not moved to {STATE_DIR}/ (C3 never moves a folder through a link); its target is untouched and a fresh {STATE_DIR}/ is used",
            crate::setup::repo_models::link_words(&eck)
        ));
        return out;
    }
    if !meta.is_dir() {
        return out;
    }
    if let Some(why) = live_run_under(repo_root, &eck) {
        out.deferred = Some(why);
        return out;
    }
    let renamed = if c3_core::test_hooks::hook(RENAME_FAIL_HOOK).is_some() {
        Err(std::io::Error::other(format!(
            "simulated by {RENAME_FAIL_HOOK}"
        )))
    } else {
        std::fs::rename(&eck, &c3)
    };
    if let Err(e) = renamed {
        out.deferred = Some(format!(
            "the rename of {LEGACY_STATE_DIR}/ to {STATE_DIR}/ failed: {e}; nothing else was tried"
        ));
        return out;
    }
    out.moved = true;
    let notes = &mut out.notes;
    notes.push(format!(
        "moved {LEGACY_STATE_DIR}/ to {STATE_DIR}/ - C3's project folder is now {STATE_DIR}/"
    ));

    // The old tool's leftovers at the top of the folder.
    let legacy = c3.join(LEGACY_SUBDIR);
    let mut moved: Vec<&str> = Vec::new();
    for name in LEGACY_LEFTOVERS {
        let src = c3.join(name);
        if std::fs::symlink_metadata(&src).is_err() {
            continue;
        }
        match std::fs::symlink_metadata(&legacy) {
            Err(_) => {
                if let Err(e) = std::fs::create_dir(&legacy) {
                    notes.push(format!(
                        "left {STATE_DIR}/{name}: could not create {STATE_DIR}/{LEGACY_SUBDIR}/ ({e})"
                    ));
                    continue;
                }
            }
            Ok(m) if m.is_dir() && !crate::setup::repo_models::is_link(&m) => {}
            Ok(_) => {
                notes.push(format!(
                    "left {STATE_DIR}/{name}: {STATE_DIR}/{LEGACY_SUBDIR} is not a plain directory"
                ));
                continue;
            }
        }
        let dst = legacy.join(name);
        if std::fs::symlink_metadata(&dst).is_ok() {
            notes.push(format!(
                "left {STATE_DIR}/{name}: {STATE_DIR}/{LEGACY_SUBDIR}/{name} exists already"
            ));
            continue;
        }
        match std::fs::rename(&src, &dst) {
            Ok(()) => moved.push(name),
            Err(e) => notes.push(format!(
                "left {STATE_DIR}/{name}: could not move it to {STATE_DIR}/{LEGACY_SUBDIR}/ ({e})"
            )),
        }
    }
    if !moved.is_empty() {
        notes.push(format!(
            "moved the old tool's {} to {STATE_DIR}/{LEGACY_SUBDIR}/ (C3 never reads them)",
            moved.join(", ")
        ));
    }

    // The old tool's report becomes `last-report.md`.
    let report = c3.join(LEGACY_REPORT[0]).join(LEGACY_REPORT[1]);
    if std::fs::symlink_metadata(&report).is_ok_and(|m| m.is_file()) {
        let dst = c3.join(LAST_REPORT);
        if std::fs::symlink_metadata(&dst).is_ok() {
            notes.push(format!(
                "left {STATE_DIR}/{}/{}: {STATE_DIR}/{LAST_REPORT} exists already",
                LEGACY_REPORT[0], LEGACY_REPORT[1]
            ));
        } else {
            match std::fs::rename(&report, &dst) {
                Ok(()) => notes.push(format!(
                    "moved {STATE_DIR}/{}/{} to {STATE_DIR}/{LAST_REPORT}",
                    LEGACY_REPORT[0], LEGACY_REPORT[1]
                )),
                Err(e) => notes.push(format!(
                    "left {STATE_DIR}/{}/{}: could not move it to {STATE_DIR}/{LAST_REPORT} ({e})",
                    LEGACY_REPORT[0], LEGACY_REPORT[1]
                )),
            }
        }
    }

    match ensure_gitignore(repo_root) {
        Ok(true) => notes.push(format!(
            "added `{GITIGNORE_LINE}` to .gitignore (the `{LEGACY_STATE_DIR}/` line is left as it is)"
        )),
        Ok(false) => {}
        Err(e) => notes.push(format!("`{GITIGNORE_LINE}` not added to .gitignore: {e}")),
    }
    out
}

/// (S1 point 11) Why the legacy folder may not be moved now: a detached consultation (its status
/// records), a work run (its `run.json` with a live owner) or an interrupted consultation (an
/// active recovery record) of the repository is live in a task directory under `dir` (searched
/// three levels deep, never through a link). `None` when nothing is live - in doubt (a record that
/// cannot be read) the rename is tried and a failure reads as "not now".
pub fn live_run_under(repo_root: &Path, dir: &Path) -> Option<String> {
    fn walk(repo_root: &Path, d: &Path, depth: usize) -> Option<String> {
        let rd = std::fs::read_dir(d).ok()?;
        let mut subdirs: Vec<PathBuf> = Vec::new();
        for e in rd.flatten() {
            let Ok(m) = std::fs::symlink_metadata(e.path()) else {
                continue;
            };
            if m.is_dir() && !crate::setup::repo_models::is_link(&m) {
                subdirs.push(e.path());
            }
        }
        subdirs.sort();
        for task_dir in subdirs {
            if task_is_live(&task_dir) {
                return Some(format!(
                    "a run is live under {}",
                    shown(repo_root, &task_dir)
                ));
            }
            let name = task_dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            // not a place where task directories live
            let skip = [
                "snapshots",
                "lastsnapshot",
                "scouts",
                "index",
                "explains",
                "legacy",
            ]
            .contains(&name.as_str())
                || name == "work"
                || name == "handoffs";
            if depth > 1 && !skip {
                if let Some(why) = walk(repo_root, &task_dir, depth - 1) {
                    return Some(why);
                }
            }
        }
        None
    }
    walk(repo_root, dir, 3)
}

/// Whether one task directory holds a live run (see [`live_run_under`]).
fn task_is_live(task_dir: &Path) -> bool {
    if crate::consult::detach::has_live_detached_run(task_dir) {
        return true;
    }
    // the task directory's own recovery records (a kill copy in the codex home holds nothing open
    // under the folder)
    for p in crate::liveness::pending::pending_paths_with(task_dir, None).paths {
        let rd = crate::liveness::pending::read_pending_file(&p);
        if let Some(record) = rd.record {
            if crate::liveness::pending::test_pending_active(&record, &p).active {
                return true;
            }
        }
    }
    let work_root = task_dir.join("work");
    if let Ok(dirs) = crate::work::record::run_dirs(&work_root) {
        for (_, run_dir) in dirs {
            if let Ok(Some(rec)) = crate::work::record::read_record(&run_dir) {
                if crate::work::record::is_live(&rec) {
                    return true;
                }
            }
        }
    }
    false
}

/// Which of the two folders a path starts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StateName {
    Current,
    Legacy,
}

/// The folder a path lies in (`.c3` or `.eck` at the repository root, compared the way the file
/// system compares names) and the rest of the path below it (`/`-separated, `.` steps dropped).
fn split_state_path(repo_root: &Path, p: &Path) -> Option<(StateName, String)> {
    let rel = c3_core::paths::repo_relative(repo_root, p)?;
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

/// (S1 point 11) The collab directory a command uses, from the path it was handed (`given`,
/// already joined to the repository root when it was relative). Every command that resolves a
/// collab directory goes through this ([`crate::providers::resolve_collab_root`] and
/// `c3 work`'s paths): CLI `--collab-dir`, the MCP `collab_dir`, the hook's look for detached
/// runs, `c3 providers`' ledger scan.
///
/// * a path inside `.eck/` that does not exist while `.c3` does (or neither folder exists - a
///   collab path never creates a new `.eck/`): the same relative path under `.c3/`, said once;
/// * a path inside `.c3/` while `.c3` does not exist and `.eck/` does: the one-time move first
///   ([`migrate_legacy`]; never by the hook, never inside a C3 child); when it cannot be made now
///   (or it is not made), the legacy path is used for this run and said once;
/// * a path inside `.eck/` while `.eck/` exists and `.c3` does not: as given (only the writing
///   commands move the folder);
/// * with the switch off ([`migration_enabled`]), and for every other path: exactly as given.
pub fn collab_dir(repo_root: &Path, given: PathBuf) -> PathBuf {
    let Some((kind, rest)) = split_state_path(repo_root, &given) else {
        return given;
    };
    if !migration_enabled() {
        return given;
    }
    let c3 = path(repo_root);
    let eck = legacy_path(repo_root);
    let c3_exists = std::fs::symlink_metadata(&c3).is_ok();
    match kind {
        StateName::Legacy => {
            if std::fs::symlink_metadata(&given).is_ok() || (!c3_exists && eck.is_dir()) {
                return given;
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
                return given;
            }
            let legacy = join_rest(&eck, &rest);
            if NO_MOVE.load(std::sync::atomic::Ordering::SeqCst) {
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
                    // moved - or a link left alone, whose fresh `.c3/` is used
                    None => return given,
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

/// Move a legacy cache `<collab_root>/.c3/` (C3's `anchor`, `snapshot_seq`, `index/`, ...) into
/// `<repo>/.c3/`, entry by entry, when the destination entry does not exist: an existing
/// destination wins and the source entry is left and named in a note. Nothing moves when the
/// collab root IS the repository root (the same folder), when either folder is a link, or inside a
/// C3 child (the caller's rule). The emptied source folder is removed. Returns the notes.
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
    if crate::setup::repo_models::is_link(&meta) || !meta.is_dir() {
        if crate::setup::repo_models::is_link(&meta) {
            notes.push(format!(
                "{src_label} is {} - not moved into {STATE_DIR}/",
                crate::setup::repo_models::link_words(&src)
            ));
        }
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
            if let Err(e) = std::fs::create_dir_all(&dst) {
                notes.push(format!(
                    "{src_label} not moved: could not create {STATE_DIR}/ ({e})"
                ));
                return notes;
            }
        }
        Ok(m) if m.is_dir() && !crate::setup::repo_models::is_link(&m) => {}
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
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "c3-statedir-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn state_components_compare_like_the_file_system() {
        assert!(is_state_component(".c3"));
        assert!(is_state_component(".eck"));
        assert!(!is_state_component(".collab"));
        assert!(!is_state_component("c3"));
        let insensitive = cfg!(any(windows, target_os = "macos"));
        assert_eq!(is_state_component(".C3"), insensitive);
        assert_eq!(is_state_component(".Eck"), insensitive);
    }

    #[test]
    fn a_gitignore_line_that_ignores_the_folder_counts() {
        let r = scratch("gi");
        for body in [".c3/\n", ".c3\n", "/.c3/\n", ".*\n", "**/.c3\n"] {
            assert!(gitignore_ignores_state_dir(&r, body), "{body:?}");
        }
        for body in ["", ".eck/\n", "target/\n", ".c3/x\n", ".*\n!.c3/\n"] {
            assert!(!gitignore_ignores_state_dir(&r, body), "{body:?}");
        }
        let _ = std::fs::remove_dir_all(&r);
    }

    // `.c3ignore` wins over `.eckignore` in the same directory; a directory with only `.eckignore`
    // is still honoured; the deeper file decides.
    #[test]
    fn c3ignore_wins_and_eckignore_is_the_fallback() {
        let r = scratch("ignore");
        std::fs::write(r.join(LEGACY_IGNORE_FILE), "legacy.md\n").unwrap();
        let mut ci = ContextIgnore::new(&r);
        assert!(ci.excluded("legacy.md", false));
        assert!(!ci.excluded("other.md", false));
        std::fs::write(r.join(IGNORE_FILE), "other.md\n").unwrap();
        let mut ci = ContextIgnore::new(&r);
        assert!(ci.excluded("other.md", false));
        assert!(
            !ci.excluded("legacy.md", false),
            ".eckignore is not read next to a .c3ignore"
        );
        std::fs::create_dir_all(r.join("sub")).unwrap();
        std::fs::write(r.join("sub").join(LEGACY_IGNORE_FILE), "!other.md\nx/\n").unwrap();
        let mut ci = ContextIgnore::new(&r);
        assert!(!ci.excluded("sub/other.md", false));
        assert!(ci.excluded("sub/x/deep.rs", false));
        assert!(ci.excluded("sub/x", true));
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn the_read_path_falls_back_to_the_legacy_folder() {
        let r = scratch("read");
        assert_eq!(read_path(&r), None);
        assert_eq!(layout(&r), Layout::Absent);
        std::fs::create_dir_all(legacy_path(&r)).unwrap();
        assert_eq!(read_path(&r), Some(legacy_path(&r)));
        assert_eq!(layout(&r), Layout::LegacyOnly);
        std::fs::create_dir_all(path(&r)).unwrap();
        assert_eq!(read_path(&r), Some(path(&r)));
        assert_eq!(layout(&r), Layout::Both);
        std::fs::remove_dir_all(legacy_path(&r)).unwrap();
        assert_eq!(layout(&r), Layout::Current);
        assert_eq!(label(&path(&r), "ROADMAP.md"), ".c3/ROADMAP.md");
        let _ = std::fs::remove_dir_all(&r);
    }

    // The legacy collab cache moves entry by entry; an existing destination wins and is named.
    #[test]
    fn a_legacy_collab_cache_moves_entry_by_entry() {
        let r = scratch("cache");
        let collab = r.join(".collab");
        std::fs::create_dir_all(collab.join(STATE_DIR).join("index")).unwrap();
        std::fs::write(collab.join(STATE_DIR).join("anchor"), "abc\n").unwrap();
        std::fs::write(collab.join(STATE_DIR).join("snapshot_seq"), "3").unwrap();
        std::fs::create_dir_all(path(&r)).unwrap();
        std::fs::write(path(&r).join("snapshot_seq"), "9").unwrap();
        let notes = migrate_collab_cache(&r, &collab);
        assert_eq!(
            std::fs::read_to_string(path(&r).join("anchor")).unwrap(),
            "abc\n"
        );
        assert!(path(&r).join("index").is_dir());
        assert_eq!(
            std::fs::read_to_string(path(&r).join("snapshot_seq")).unwrap(),
            "9"
        );
        assert!(collab.join(STATE_DIR).join("snapshot_seq").is_file());
        assert!(
            notes
                .iter()
                .any(|n| n.contains("left .collab/.c3/snapshot_seq")),
            "{notes:?}"
        );
        assert!(
            notes.iter().any(|n| n.contains("anchor, index")),
            "{notes:?}"
        );
        // the collab root IS the repository root: the same folder, nothing moves
        assert!(migrate_collab_cache(&r, &r).is_empty());
        // (point 11, e) a collab root inside the folder: `.c3/collab/.c3/*` moves to `.c3/*`
        let inner = path(&r).join("collab").join(STATE_DIR);
        std::fs::create_dir_all(inner.join("explains")).unwrap();
        let notes = migrate_collab_cache(&r, &path(&r).join("collab"));
        assert!(path(&r).join("explains").is_dir(), "{notes:?}");
        assert!(!inner.exists(), "the emptied source is removed");
        let _ = std::fs::remove_dir_all(&r);
    }

    // (S1 point 11) A work run whose owner is alive under the legacy folder blocks its move; a
    // finished one does not.
    #[test]
    fn a_live_work_run_under_the_legacy_folder_is_found() {
        let r = scratch("live");
        let run_dir = legacy_path(&r).join("collab/t/work/01-zai-glm");
        std::fs::create_dir_all(&run_dir).unwrap();
        let mut rec = crate::work::record::RunRecord {
            id: "01-zai-glm".into(),
            task: "t".into(),
            seq: 1,
            status: "running".into(),
            started: "2026-11-23T10:00:00+00:00".into(),
            pid: Some(std::process::id() as i64),
            ..Default::default()
        };
        crate::work::record::write_record(&run_dir, &rec).unwrap();
        let why = live_run_under(&r, &legacy_path(&r)).expect("live");
        assert!(why.contains("a run is live under .eck/collab/t"), "{why}");
        rec.status = "done".into();
        crate::work::record::write_record(&run_dir, &rec).unwrap();
        assert_eq!(live_run_under(&r, &legacy_path(&r)), None);
        let _ = std::fs::remove_dir_all(&r);
    }

    // (S1 point 11) A path inside either folder is split into the folder and the rest; others are not.
    #[test]
    fn collab_paths_are_split_by_their_folder() {
        let r = scratch("split");
        assert_eq!(
            split_state_path(&r, &r.join(".eck").join("collab")),
            Some((StateName::Legacy, "collab".to_string()))
        );
        assert_eq!(
            split_state_path(&r, &r.join(".").join(".c3").join("collab").join("x")),
            Some((StateName::Current, "collab/x".to_string()))
        );
        assert_eq!(split_state_path(&r, &r.join(".collab")), None);
        assert_eq!(split_state_path(&r, Path::new("/elsewhere/.eck")), None);
        let _ = std::fs::remove_dir_all(&r);
    }
}
