//! Repair a repository's model settings in place (follow-up to `c3 doctor`): C3 rewrites a
//! stale agent model pin to the floating alias, and lets `AGENTS.md` (the OpenCode agent file
//! eckSnapshot writes) follow the roster's successor. It makes exactly two kinds of edit, each
//! changing ONE value of a file and nothing else (every other byte - the quotes around the value,
//! a trailing comment, the line endings, a BOM - is preserved). Nothing here runs git or any other
//! process, commits, or writes outside the repository root; the edits are left uncommitted for
//! `c3 finish`.
//!
//! (0.2.5, F02-1 / F02-2) A file is never written through: the new content goes to a sibling
//! temporary file in the same directory, which is read back, the original is re-read immediately
//! before the rename and compared with the snapshot the edit was computed from (when it changed,
//! the repair is dropped and the file left as it is), and the temporary file then REPLACES the
//! original by rename - so a failed or interrupted repair never leaves a truncated file, and a
//! second name of the file (a hard link) never sees the edit. A file with more than one hard link,
//! a link (symlink, junction) or another reparse point, and a file under a linked `.claude` /
//! `.claude/agents` are not edited at all; the file is checked again immediately before the rename.
//! Every repair that was not made is reported with its reason.
//!
//! (0.2.5 D4, F13-9 - the "decided boundary" is withdrawn) Every step is relative to directory
//! HANDLES: the repository root is opened once ([`crate::contain::Root::open_anchor`]), each
//! directory on the way to the file is opened relative to its parent's handle without following it
//! (a link refuses), the file is checked, read and re-read through handles opened without following
//! it, and the temporary file is created, renamed onto the file and - on a failure - removed
//! relative to the file's directory handle ([`crate::contain::publish`]). A directory swapped for a
//! link between the last check and the rename cannot carry the replacement out of the repository.
//!
//! It is used from two places: `c3 doctor --fix` (which then re-reports what is left) and the
//! `c3 hook` SessionStart line (automatic, default ON, off with `C3_REPO_MODELS=off` or
//! `"repo_models": false` in `<codex home>/c3/config.json`). Inside a child C3 started
//! (`C3_CHILD`) it does nothing at all.

use std::path::{Path, PathBuf};

use c3_core::claude::model_family;
use c3_core::roster::Roster;

use crate::maintain::models::{dated_older, ModelsState};
use crate::maintain::version::{parse_model_id, successor};
use crate::setup::doctor::{agent_model, frontmatter_model, pinned_agent_model};

/// One model line C3 rewrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFix {
    /// Repo-relative path of the edited file.
    pub file: String,
    /// The model id before the edit.
    pub old: String,
    /// The model id after the edit.
    pub new: String,
}

/// One repair C3 did NOT make, and why (a link, a second hard link, a concurrent edit, a failed
/// write or replace). The file is as it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairProblem {
    /// Repo-relative path of the file.
    pub file: String,
    /// The model id it still names.
    pub old: String,
    /// The value the repair would have written.
    pub new: String,
    /// Why it was not written.
    pub why: String,
}

/// What a repair pass did: the fixes, the repairs it could not make, and whether it did nothing
/// because it runs inside a child C3 started.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepairReport {
    pub fixes: Vec<ModelFix>,
    pub problems: Vec<RepairProblem>,
    /// `C3_CHILD` is set: nothing was read or written ([`SKIPPED_IN_CHILD`]).
    pub skipped_in_child: bool,
}

/// How a pass that did nothing inside a C3 child is reported.
pub const SKIPPED_IN_CHILD: &str = "skipped inside a C3 child";

/// Replace the value of the top-level `model` key in a YAML frontmatter with `new_value`, keeping
/// every other byte of `content` (the BOM, the line endings, the key spelling, the quotes around
/// the value, a trailing comment, every other line). The key and the value span are the ones
/// [`frontmatter_model`] reads - the same rule `c3 doctor` reports by. `None` without the key.
fn replace_frontmatter_model_value(content: &str, new_value: &str) -> Option<String> {
    let line = frontmatter_model(content)?;
    let mut out = String::with_capacity(content.len() + new_value.len());
    out.push_str(&content[..line.span.start]);
    out.push_str(new_value);
    out.push_str(&content[line.span.end..]);
    Some(out)
}

/// The rewrite for a `.claude/agents/*.md` file: a frontmatter `model` that is a validated
/// CONCRETE Claude id (the engine's model table or the published shape) becomes its family's
/// alias, a `[1m]` context modifier kept (`claude-opus-4-8` -> `opus`, `claude-opus-4-8[1m]` ->
/// `opus[1m]`). A custom `claude-opus-foo`, an alias with a modifier (`opus[1m]`) or any other
/// value is left alone (doctor keeps reporting a pin it cannot name an alias for). Returns
/// `(new_content, old, new)`.
pub fn rewrite_agent_model(content: &str) -> Option<(String, String, String)> {
    let (old, alias) = pinned_agent_model(content)?;
    if alias.is_empty() {
        return None;
    }
    let new_content = replace_frontmatter_model_value(content, &alias)?;
    Some((new_content, old, alias))
}

/// The rewrite for the repository-root `AGENTS.md`: its frontmatter `model` follows the roster.
/// `roster_models` holds the current model of each ACTIVE roster entry that may be followed (not
/// protected, not on request: [`roster_models`]), one element per entry. (0.2.5, F02-4) The value
/// is bound to those entries one by one, never to the highest id of all of them:
///
/// * an entry whose model IS the value (case-insensitively) keeps it: `AGENTS.md` is in step;
/// * otherwise each entry's model is asked on its own whether it is the value's successor
///   ([`successor`]: same prefix and suffix, a higher version - a higher major included);
/// * (0.2.5 R2, F13-6) a successor the maintain state dates as NOT newer than the value is never
///   followed ([`dated_older`]: a public catalog of `models_state` dates both ids and the
///   candidate not later, or an earlier pass recorded the pair as older - `grok-4.20` is a 2025
///   model below `grok-4.7` although 4.20 > 4.7); without dates for both ids the version rule
///   alone decides, as before;
/// * the value is rewritten only when exactly ONE successor id results; two entries carrying
///   different successors leave it alone (ambiguous).
///
/// The new value keeps the casing of the original prefix/suffix around the successor's version
/// (`GLM-5.3` -> `GLM-5.4`, `Qwen3.8-Max` -> `Qwen4-Max`). A Claude-family value is the agent-file
/// rule's, not this one. Returns `(new_content, old, new)`.
pub fn rewrite_agents_md_model(
    content: &str,
    roster_models: &[String],
    models_state: &ModelsState,
) -> Option<(String, String, String)> {
    let value = agent_model(content)?;
    if value.trim().is_empty() || !model_family(&value).is_empty() {
        return None;
    }
    let lowered = value.to_lowercase();
    let entries: Vec<String> = roster_models
        .iter()
        .map(|m| m.trim().to_lowercase())
        .filter(|m| !m.is_empty())
        .collect();
    if entries.contains(&lowered) {
        return None;
    }
    let mut succs: Vec<&str> = Vec::new();
    for m in &entries {
        if let Some(s) = successor(&lowered, std::slice::from_ref(m)) {
            if !succs.contains(&s) && !dated_older(models_state, &lowered, s) {
                succs.push(s);
            }
        }
    }
    let [succ] = succs.as_slice() else {
        return None;
    };
    // Reconstruct with the ORIGINAL casing: the original prefix/suffix around the successor's
    // version as the successor spells it (`GLM-5.3` + `glm-6` -> `GLM-6`, + `glm-5.4` -> `GLM-5.4`).
    let orig = parse_model_id(&value)?;
    let succ_parsed = parse_model_id(succ)?;
    let succ_chars: Vec<char> = succ.chars().collect();
    let vstart = succ_parsed.prefix.chars().count();
    let vend = succ_chars
        .len()
        .checked_sub(succ_parsed.suffix.chars().count())?;
    if vend <= vstart {
        return None;
    }
    let version: String = succ_chars[vstart..vend].iter().collect();
    let new_value = format!("{}{}{}", orig.prefix, version, orig.suffix);
    if new_value == value {
        return None;
    }
    let new_content = replace_frontmatter_model_value(content, &new_value)?;
    Some((new_content, value, new_value))
}

/// Apply both kinds of fix to a repository. `roster_models` is [`roster_models`]' list (for the
/// AGENTS.md rule, whose date gate reads the maintain state's catalog dates -
/// [`crate::maintain::models_state`]). Inside a child C3 started (`C3_CHILD`, F04-4) it does
/// nothing and says so ([`RepairReport::skipped_in_child`]). Otherwise every file is replaced,
/// never written through (see the module doc); a repair that cannot be made safely is reported,
/// never forced. Nothing is written outside `repo_root` and no process is started.
pub fn refresh_repo_models(repo_root: &Path, roster_models: &[String]) -> RepairReport {
    if crate::engines::in_c3_child() {
        return RepairReport {
            skipped_in_child: true,
            ..RepairReport::default()
        };
    }
    refresh_repo_models_at(repo_root, roster_models, &crate::maintain::models_state())
}

/// [`refresh_repo_models`] without the C3-child check, with the maintain state given.
fn refresh_repo_models_at(
    repo_root: &Path,
    roster_models: &[String],
    models_state: &ModelsState,
) -> RepairReport {
    let mut report = RepairReport::default();

    // 1. `.claude/agents/*.md` pins -> the alias. The directory is read even when it is a link
    //    (reading cannot harm, and the refused repair is then reported); an edit needs `.claude`
    //    and `.claude/agents` to be real directories and the file a plain file with one name.
    let claude_dir = repo_root.join(".claude");
    let agents = claude_dir.join("agents");
    let mut names: Vec<std::ffi::OsString> = std::fs::read_dir(&agents)
        .map(|rd| rd.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default();
    names.sort();
    for name in names {
        let name = name.to_string_lossy().to_string();
        if !name.to_lowercase().ends_with(".md") {
            continue;
        }
        let path = agents.join(&name);
        let Ok(snapshot) = std::fs::read(&path) else {
            continue;
        };
        let Ok(content) = std::str::from_utf8(&snapshot) else {
            continue;
        };
        if let Some((new_content, old, new)) = rewrite_agent_model(content) {
            let edit = Edit {
                file: format!(".claude/agents/{name}"),
                root: repo_root,
                snapshot: &snapshot,
                new_content: new_content.as_bytes(),
            };
            apply(&mut report, &edit, old, new);
        }
    }

    // 2. The repository-root `AGENTS.md` -> the roster's successor (the same safety rules).
    let agents_md = repo_root.join("AGENTS.md");
    if let Ok(snapshot) = std::fs::read(&agents_md) {
        if let Ok(content) = std::str::from_utf8(&snapshot) {
            if let Some((new_content, old, new)) =
                rewrite_agents_md_model(content, roster_models, models_state)
            {
                let edit = Edit {
                    file: "AGENTS.md".to_string(),
                    root: repo_root,
                    snapshot: &snapshot,
                    new_content: new_content.as_bytes(),
                };
                apply(&mut report, &edit, old, new);
            }
        }
    }

    report
}

/// One planned edit: the file (repository-relative, `/`-separated - every directory on its way
/// must be a real directory, named in a refusal by its repository-relative path), the repository
/// root, the bytes the edit was computed from and the new bytes.
pub(crate) struct Edit<'a> {
    pub(crate) file: String,
    pub(crate) root: &'a Path,
    pub(crate) snapshot: &'a [u8],
    pub(crate) new_content: &'a [u8],
}

/// Make one edit (or record why not) in `report`.
fn apply(report: &mut RepairReport, edit: &Edit<'_>, old: String, new: String) {
    let result = match edit_refusal(edit) {
        Some(why) => Err(why),
        None => replace_file(edit),
    };
    match result {
        Ok(()) => report.fixes.push(ModelFix {
            file: edit.file.clone(),
            old,
            new,
        }),
        Err(why) => report.problems.push(RepairProblem {
            file: edit.file.clone(),
            old,
            new,
            why,
        }),
    }
}

/// (0.2.5 D4) The directory of `edit.file`, reached from the repository root's handle one
/// component at a time without following any ([`crate::contain::open_child`]), and the file's name;
/// `Err` is the refusal: a directory on the way that is a link, a reparse point or no directory.
fn open_edit_dir(edit: &Edit<'_>) -> Result<(cap_std::fs::Dir, std::ffi::OsString), String> {
    let root = crate::contain::Root::open_anchor(edit.root)
        .map_err(|e| format!("the repository root {} - not edited", e.words()))?;
    let parts: Vec<&str> = edit.file.split('/').filter(|p| !p.is_empty()).collect();
    let Some((leaf, dirs)) = parts.split_last() else {
        return Err("it names no file - not edited".to_string());
    };
    let mut dir = root
        .dir()
        .try_clone()
        .map_err(|e| format!("the repository root cannot be opened ({e}) - not edited"))?;
    let mut label = String::new();
    for d in dirs {
        if !label.is_empty() {
            label.push('/');
        }
        label.push_str(d);
        dir = crate::contain::open_child(&dir, std::ffi::OsStr::new(d), false).map_err(|why| {
            format!(
                "{label} is {} or no directory, so the file may lie outside the repository - not edited",
                step_link_words(&why)
            )
        })?;
    }
    Ok((dir, std::ffi::OsString::from(leaf)))
}

/// [`link_words`] from a refused step: a link names what it is, anything else both words.
fn step_link_words(why: &crate::contain::Step) -> &'static str {
    match why {
        crate::contain::Step::Link { symlink } => reparse_words(*symlink, true),
        _ => reparse_words(false, false),
    }
}

/// (0.2.5 D4) The file `leaf` of `dir` opened without following it and checked on that handle: a
/// regular file that is not a link or a reparse point, with exactly one name. Its handle is
/// returned (for reading); `Err` is the refusal.
fn open_edit_file(
    dir: &cap_std::fs::Dir,
    leaf: &std::ffi::OsStr,
) -> Result<cap_std::fs::File, String> {
    let f = crate::contain::open_file_in(dir, leaf).map_err(|why| {
        format!(
            "it is {} or no regular file, so its target may lie outside the repository - not edited",
            step_link_words(&why)
        )
    })?;
    let links = f
        .metadata()
        .map(|m| cap_fs_ext::MetadataExt::nlink(&m))
        .map_err(|e| format!("could not count its hard links ({e}) - not edited"))?;
    match links {
        1 => Ok(f),
        n => Err(format!(
            "it has {n} hard links, so an edit would change the other names too - not edited"
        )),
    }
}

/// Why `edit.file` may not be edited, or `None` when it may: every directory on its way must be a
/// real directory (not a symlink or junction), the file a regular file that is not a link, with
/// exactly one name (a second hard link would see the edit too). (0.2.5 D4) Read through handles
/// from the repository root, nothing followed.
pub(crate) fn edit_refusal(edit: &Edit<'_>) -> Option<String> {
    open_edit_dir(edit)
        .and_then(|(dir, leaf)| open_edit_file(&dir, &leaf).map(|_| ()))
        .err()
}

/// Replace `edit.file` with `edit.new_content` without writing through it: a sibling temporary
/// file is written, synced and read back; the file is checked again and re-read and compared with
/// `edit.snapshot`; then the temporary file is renamed over the original. On any failure the
/// temporary file is removed (a removal that fails is named in the `Err`) and the original is
/// untouched. (0.2.5 D4, F13-9) All of it relative to the file's directory HANDLE, reached from the
/// repository root without following any component ([`open_edit_dir`], [`crate::contain::publish`]).
pub(crate) fn replace_file(edit: &Edit<'_>) -> Result<(), String> {
    use std::io::Read;
    let (dir, leaf) = open_edit_dir(edit)?;
    let original = open_edit_file(&dir, &leaf)?;
    // the replacement keeps the original's permission bits (Unix; on Windows only the read-only
    // attribute would carry over, which the repair never had to keep)
    #[cfg(unix)]
    let permissions = original.metadata().ok().map(|m| m.permissions());
    #[cfg(not(unix))]
    let permissions = None;
    drop(original);
    let name = leaf.to_string_lossy().to_string();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = format!(".{name}.c3-repair-{}-{nanos}.tmp", std::process::id());
    let tmp_os = std::ffi::OsString::from(&tmp);
    let check = |stage: crate::contain::Stage| -> Result<(), String> {
        if stage == crate::contain::Stage::BeforeCreate {
            return Ok(());
        }
        match crate::contain::read_file_in(&dir, &tmp_os) {
            Ok(b) if b == edit.new_content => {}
            Ok(_) => {
                return Err(
                    "the temporary file did not read back as written - left as it was".to_string(),
                )
            }
            Err(e) => {
                return Err(format!(
                    "could not read the temporary file back ({e}) - left as it was"
                ))
            }
        }
        // Immediately before the replace: the file once more, then its content against the
        // snapshot the edit was computed from (an editor's save in between wins; the repair is
        // dropped).
        let mut f = open_edit_file(&dir, &leaf)?;
        let mut now = Vec::new();
        f.read_to_end(&mut now)
            .map_err(|e| format!("could not re-read it ({e}) - left as it was"))?;
        if now != edit.snapshot {
            return Err("it changed while it was being repaired - left as it was".to_string());
        }
        crate::contain::race_point("repair-replace");
        Ok(())
    };
    crate::contain::publish(&crate::contain::Publish {
        dir: &dir,
        leaf: &leaf,
        tmp: &tmp_os,
        bytes: edit.new_content,
        permissions,
        check: &check,
        race: "repair",
    })
    .map_err(|f| {
        let msg = match &f.error {
            crate::contain::PublishError::Check(why) => why.clone(),
            crate::contain::PublishError::Create(e) => {
                format!("could not create the temporary file ({e}) - left as it was")
            }
            crate::contain::PublishError::Write(e) => {
                format!("could not write the temporary file ({e}) - left as it was")
            }
            crate::contain::PublishError::Rename(e) => {
                format!("could not replace it ({e}) - left as it was")
            }
        };
        f.with_cleanup(msg)
    })
}

/// (0.2.5 R2, F12-3) How a refusal names what `p` is, read without following it: `a link (a
/// symlink or junction)` when it is one, `a reparse point (not a symlink or junction - e.g. a
/// cloud-sync placeholder)` for any other Windows reparse point, which C3 refuses too since it
/// cannot tell where it leads; otherwise both words (the entry is no link: missing, or another
/// kind - the rest of the sentence says so).
pub(crate) fn link_words(p: &Path) -> &'static str {
    let m = std::fs::symlink_metadata(p).ok();
    let symlink = m.as_ref().is_some_and(|m| m.file_type().is_symlink());
    let reparse = m.as_ref().is_some_and(is_link);
    reparse_words(symlink, reparse)
}

/// [`link_words`] from what the metadata says: a symlink (Rust's `is_symlink`, on Windows a
/// name-surrogate reparse point - a symlink or a junction), and a reparse point at all.
fn reparse_words(symlink: bool, reparse: bool) -> &'static str {
    match (symlink, reparse) {
        (true, _) => "a link (a symlink or junction)",
        (false, true) => {
            "a reparse point (not a symlink or junction - e.g. a cloud-sync placeholder)"
        }
        (false, false) => "a link (a symlink or junction), a reparse point",
    }
}

/// Whether the metadata (read without following a link) is a link: a symlink, or on Windows any
/// reparse point (a junction included).
pub(crate) fn is_link(m: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        m.file_type().is_symlink() || m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        m.file_type().is_symlink()
    }
}

/// The roster entries `AGENTS.md` may follow: the model id of each ACTIVE entry that is neither
/// protected (`ext.reserve_percent`) nor seated only on request (`ext.on_request`), one element per
/// entry. Empty on any error.
pub fn roster_models(roster: &Roster) -> Vec<String> {
    if !roster.exists {
        return Vec::new();
    }
    roster
        .active_entries()
        .filter(|e| e.reserve_percent <= 0 && !e.on_request)
        .map(|e| e.model.clone())
        .filter(|m| !m.is_empty())
        .collect()
}

/// Whether the automatic repository model refresh (the `c3 hook` action) is ON. Default ON;
/// `C3_REPO_MODELS=off` (environment wins) or `"repo_models": false` in the C3 config turns it
/// off. Mirrors [`crate::maintain::enabled`] / [`crate::update::check_enabled`].
pub fn enabled() -> bool {
    if let Ok(v) = std::env::var("C3_REPO_MODELS") {
        if v.trim().eq_ignore_ascii_case("off") {
            return false;
        }
        return true;
    }
    match crate::c3config::load() {
        Ok(Some(cfg)) => cfg.repo_models,
        _ => true,
    }
}

/// (0.2.5, F02-6) The repository root WITHOUT running git: the nearest directory, from `start`
/// upwards, holding a `.git` entry. (R2, F13-2) That entry decides: the directory is the root only
/// when its `.git` is a repository ([`is_git_entry`]: a directory holding `HEAD`, or the `gitdir:`
/// file of a linked worktree or a submodule whose target directory holds `HEAD`); a stray `.git`
/// (an empty file, a broken link, a non-repository directory) means no root - `None`, and the
/// repair does nothing. `None` too when no ancestor has a `.git` entry.
pub fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let dotgit = d.join(".git");
        if std::fs::symlink_metadata(&dotgit).is_ok() {
            return is_git_entry(&dotgit).then(|| d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

/// Whether a `.git` entry is a repository, read without running git and without following a link:
/// a real directory (not a link) that holds `HEAD`, or a regular file (not a link) whose first line
/// is `gitdir: <path>` naming a directory - relative paths against the file's own directory - that
/// holds `HEAD`. Anything else is not.
fn is_git_entry(dotgit: &Path) -> bool {
    let Ok(m) = std::fs::symlink_metadata(dotgit) else {
        return false;
    };
    if is_link(&m) {
        return false;
    }
    if m.file_type().is_dir() {
        return dotgit.join("HEAD").is_file();
    }
    if !m.file_type().is_file() {
        return false;
    }
    let mut text = String::new();
    let read = std::fs::File::open(dotgit).and_then(|f| {
        use std::io::Read;
        f.take(4096).read_to_string(&mut text)
    });
    if read.is_err() {
        return false;
    }
    let Some(target) = text
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("gitdir:"))
        .map(str::trim)
        .filter(|t| !t.is_empty())
    else {
        return false;
    };
    let target = Path::new(target);
    let target = if target.is_absolute() {
        target.to_path_buf()
    } else {
        match dotgit.parent() {
            Some(base) => base.join(target),
            None => return false,
        }
    };
    target.is_dir() && target.join("HEAD").is_file()
}

/// The hook's line for a repair pass: what was refreshed and what could not be, in one line;
/// `None` when there is nothing to say.
pub fn hook_line_for(report: &RepairReport) -> Option<String> {
    let fixed = report
        .fixes
        .iter()
        .map(|f| format!("{} {} -> {}", f.file, f.old, f.new))
        .collect::<Vec<_>>()
        .join("; ");
    let failed = report
        .problems
        .iter()
        .map(|p| format!("{} {} -> {}: {}", p.file, p.old, p.new, p.why))
        .collect::<Vec<_>>()
        .join("; ");
    match (fixed.is_empty(), failed.is_empty()) {
        (true, true) => None,
        (false, true) => Some(format!(
            "c3: model settings refreshed in this repository - {fixed} (left uncommitted)"
        )),
        (true, false) => Some(format!(
            "c3: model settings NOT refreshed in this repository - {failed}"
        )),
        (false, false) => Some(format!(
            "c3: model settings refreshed in this repository - {fixed} (left uncommitted); NOT refreshed - {failed}"
        )),
    }
}

/// The `c3 hook` line: apply the refresh to the current repository and return ONE line when
/// something changed or a repair could not be made, else `None`. The feature is default ON (off
/// switch in [`enabled`]). This never fails a session: every error is swallowed or reported in the
/// line, nothing is written outside the repository root, no git (no process at all) is run - the
/// root is found by walking up to `.git` ([`find_repo_root`]) - and nothing is committed. It does
/// nothing outside a git repository or when there are no agent files to fix.
///
/// It also does nothing inside a child that C3 itself started (`engines::in_c3_child`): a reviewer
/// on the `claude` engine runs this same hook in the repository under review and must stay
/// read-only, and a worker's edit would land in its patch and could break its scope check. (R2,
/// F13-2) Nor with `GIT_DIR` set in the environment: git would then take another repository than
/// the walk up to `.git` finds, so the walk's answer is not trusted.
pub fn hook_line() -> Option<String> {
    if !enabled() || crate::engines::in_c3_child() || std::env::var_os("GIT_DIR").is_some() {
        return None;
    }
    let cwd = std::env::current_dir().ok()?;
    let repo_root = find_repo_root(&cwd)?;
    let models = crate::providers::read_reviewer_roster()
        .map(|r| roster_models(&r))
        .unwrap_or_default();
    hook_line_for(&refresh_repo_models(&repo_root, &models))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "c3-repair-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A scratch repository `<base>/repo` with `.git` (holding `HEAD`) and `.claude/agents/`.
    fn repo_with_agents(tag: &str) -> (PathBuf, PathBuf) {
        let base = scratch(tag);
        let repo = base.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(repo.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::create_dir_all(repo.join(".claude").join("agents")).unwrap();
        (base, repo)
    }

    /// The names in `dir` (to prove no temporary file is left behind).
    fn names_in(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        v.sort();
        v
    }

    const PINNED: &str = "---\nname: w\nmodel: claude-opus-4-8\n---\nbody\n";

    /// The AGENTS.md rewrite with an empty maintain state (no catalog dates).
    fn rewrite_agents_md_model(c: &str, roster: &[String]) -> Option<(String, String, String)> {
        super::rewrite_agents_md_model(c, roster, &ModelsState::default())
    }

    /// The repair with an empty maintain state (no catalog dates).
    fn refresh_repo_models_at(repo: &Path, roster: &[String]) -> RepairReport {
        super::refresh_repo_models_at(repo, roster, &ModelsState::default())
    }

    /// A maintain state whose public catalog lists `(id, created)`.
    fn catalog(ids: &[(&str, Option<i64>)]) -> ModelsState {
        let mut s = crate::maintain::models::SourceList {
            source: "web:openrouter.ai".into(),
            ..Default::default()
        };
        for (id, c) in ids {
            s.ids.push(id.to_string());
            if let Some(c) = c {
                s.created.insert(id.to_string(), *c);
            }
        }
        ModelsState {
            sources: vec![s],
            ..Default::default()
        }
    }

    // (R2, F13-6) AGENTS.md never follows to a model the maintain state dates older: grok-4.20 is
    // numerically above grok-4.7 but a 2025 model. Without dates for both ids the version rule
    // alone decides, as before; a pair recorded as older by an earlier pass is refused too.
    #[test]
    fn agents_md_never_follows_to_a_model_dated_older() {
        let src = "---\nmodel: grok-4.7\n---\n";
        let roster = ["grok-4.20".to_string()];
        const Y2025: i64 = 1_740_000_000;
        const Y2026: i64 = 1_770_000_000;
        // the catalog dates both, the candidate older: no rewrite
        let dated = catalog(&[
            ("x-ai/grok-4.20", Some(Y2025)),
            ("x-ai/grok-4.7", Some(Y2026)),
        ]);
        assert!(super::rewrite_agents_md_model(src, &roster, &dated).is_none());
        // the same date counts as not newer
        let same = catalog(&[
            ("x-ai/grok-4.20", Some(Y2026)),
            ("x-ai/grok-4.7", Some(Y2026)),
        ]);
        assert!(super::rewrite_agents_md_model(src, &roster, &same).is_none());
        // dated newer: followed
        let newer = catalog(&[
            ("x-ai/grok-4.20", Some(Y2026)),
            ("x-ai/grok-4.7", Some(Y2025)),
        ]);
        let (_, _, new) = super::rewrite_agents_md_model(src, &roster, &newer).unwrap();
        assert_eq!(new, "grok-4.20");
        // only one of the two dated, or no state at all: the behaviour stays as it is
        let one = catalog(&[("x-ai/grok-4.20", Some(Y2025)), ("x-ai/grok-4.7", None)]);
        assert!(super::rewrite_agents_md_model(src, &roster, &one).is_some());
        assert!(rewrite_agents_md_model(src, &roster).is_some());
        // recorded as older in an earlier pass
        let recorded = ModelsState {
            older: vec![crate::maintain::models::OlderPair::new(
                "grok-4.7",
                "grok-4.20",
            )],
            ..Default::default()
        };
        assert!(super::rewrite_agents_md_model(src, &roster, &recorded).is_none());
        // an older-dated candidate drops out; another entry's newer one is still followed
        let roster2 = ["grok-4.20".to_string(), "grok-4.8".to_string()];
        let (_, _, new) = super::rewrite_agents_md_model(src, &roster2, &dated).unwrap();
        assert_eq!(new, "grok-4.8");
    }

    // (R2, F12-3) A refusal says "a reparse point" where the entry is a Windows reparse point that
    // is not a symlink or junction (a cloud placeholder), "a link" where it is one.
    #[test]
    fn a_refusal_names_a_reparse_point_for_what_it_is() {
        assert_eq!(reparse_words(true, true), "a link (a symlink or junction)");
        assert!(
            reparse_words(false, true).starts_with("a reparse point (not a symlink or junction")
        );
        assert!(reparse_words(false, false).contains("a reparse point"));
        let (base, repo) = repo_with_agents("words");
        let file = repo.join(".claude").join("agents").join("w.md");
        std::fs::write(&file, PINNED).unwrap();
        let edit = |file: &str| Edit {
            file: file.to_string(),
            root: &repo,
            snapshot: b"",
            new_content: b"",
        };
        // a plain directory and file: no refusal at all
        assert_eq!(edit_refusal(&edit(".claude/agents/w.md")), None);
        // a missing file: the refusal names both possibilities
        let why = edit_refusal(&edit("none.md")).unwrap();
        assert!(
            why.starts_with(
                "it is a link (a symlink or junction), a reparse point or no regular file"
            ),
            "{why}"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn agent_pin_becomes_the_alias_and_keeps_every_other_byte() {
        // CRLF line endings and a trailing body are preserved; only the model value changes.
        let src = "---\r\nname: opus-worker\r\nmodel: claude-opus-4-8\r\n---\r\nbody line\r\n";
        let (out, old, new) = rewrite_agent_model(src).unwrap();
        assert_eq!((old.as_str(), new.as_str()), ("claude-opus-4-8", "opus"));
        assert_eq!(
            out,
            "---\r\nname: opus-worker\r\nmodel: opus\r\n---\r\nbody line\r\n"
        );
        // A BOM is kept.
        let bom = "\u{feff}---\nmodel: claude-sonnet-5\n---\n";
        let (out, _, new) = rewrite_agent_model(bom).unwrap();
        assert_eq!(new, "sonnet");
        assert_eq!(out, "\u{feff}---\nmodel: sonnet\n---\n");
    }

    // (0.2.5, F03-4) Only the value span changes: the quotes, a trailing comment and the key's own
    // spelling (`model :`) survive.
    #[test]
    fn only_the_value_span_is_replaced() {
        let (out, _, _) =
            rewrite_agent_model("---\nmodel: \"claude-opus-4-8\" # pinned\n---\n").unwrap();
        assert_eq!(out, "---\nmodel: \"opus\" # pinned\n---\n");
        let (out, _, _) = rewrite_agent_model("---\nmodel : 'claude-haiku-4-5'\n---\n").unwrap();
        assert_eq!(out, "---\nmodel : 'haiku'\n---\n");
        let (out, _, _) =
            rewrite_agent_model("---\nmodel:\tclaude-sonnet-5-5   # x\r\n---\r\n").unwrap();
        assert_eq!(out, "---\nmodel:\tsonnet   # x\r\n---\r\n");
    }

    // (0.2.5, F02-3) Never a custom `claude-opus-foo`, never an alias with a modifier; a supported
    // modifier stays on the alias.
    #[test]
    fn only_a_validated_concrete_id_is_rewritten() {
        for v in [
            "claude-opus-foo",
            "opus[1m]",
            "claude-3-opus-20240229",
            "CLAUDE-OPUS-4-8",
            "opus",
            "inherit",
        ] {
            assert!(
                rewrite_agent_model(&format!("---\nmodel: {v}\n---\n")).is_none(),
                "{v}"
            );
        }
        assert!(rewrite_agent_model("---\nname: x\n---\n").is_none());
        let (out, old, new) =
            rewrite_agent_model("---\nmodel: claude-opus-4-8[1m]\n---\n").unwrap();
        assert_eq!(
            (old.as_str(), new.as_str()),
            ("claude-opus-4-8[1m]", "opus[1m]")
        );
        assert_eq!(out, "---\nmodel: opus[1m]\n---\n");
    }

    #[test]
    fn agents_md_follows_the_roster_successor_and_keeps_casing() {
        let roster = vec!["glm-5.4".to_string(), "gpt-6-astra".to_string()];
        let src = "---\nmodel: GLM-5.3\n---\nagent body\n";
        let (out, old, new) = rewrite_agents_md_model(src, &roster).unwrap();
        assert_eq!((old.as_str(), new.as_str()), ("GLM-5.3", "GLM-5.4"));
        assert_eq!(out, "---\nmodel: GLM-5.4\n---\nagent body\n");
        // Lowercase prefix stays lowercase.
        let (_, _, new) = rewrite_agents_md_model("---\nmodel: glm-5.3\n---\n", &roster).unwrap();
        assert_eq!(new, "glm-5.4");
        // The roster moved across a major: AGENTS.md follows it, the successor's spelling
        // of the version, the original casing around it.
        let (_, _, new) =
            rewrite_agents_md_model("---\nmodel: GLM-5.3\n---\n", &["glm-6.0".to_string()])
                .unwrap();
        assert_eq!(new, "GLM-6.0");
        let (_, _, new) =
            rewrite_agents_md_model("---\nmodel: Qwen3.8-Max\n---\n", &["qwen4-max".to_string()])
                .unwrap();
        assert_eq!(new, "Qwen4-Max");
    }

    #[test]
    fn agents_md_no_edit_without_a_successor() {
        // Empty roster -> nothing.
        assert!(rewrite_agents_md_model("---\nmodel: GLM-5.3\n---\n", &[]).is_none());
        // A different suffix is another family member, never followed.
        assert!(
            rewrite_agents_md_model("---\nmodel: GLM-5.3\n---\n", &["glm-6-air".to_string()])
                .is_none()
        );
        // A different family.
        assert!(
            rewrite_agents_md_model("---\nmodel: GLM-5.3\n---\n", &["qwen3.9".to_string()])
                .is_none()
        );
        // No frontmatter model line.
        assert!(rewrite_agents_md_model("---\nname: x\n---\n", &["glm-5.4".to_string()]).is_none());
        // A Claude-family value is left to the agent-file rule, not the roster rule.
        assert!(rewrite_agents_md_model(
            "---\nmodel: claude-opus-4-8\n---\n",
            &["claude-opus-5-5".to_string()]
        )
        .is_none());
    }

    // (0.2.5, F02-4) AGENTS.md is bound to the roster entries one by one: an entry still on the
    // value keeps it, two entries with different successors are ambiguous, and the highest id of
    // all entries is never taken on its own.
    #[test]
    fn agents_md_is_bound_to_one_entrys_successor() {
        let src = "---\nmodel: GLM-5.3\n---\n";
        // an entry still names the value: in step, no edit (the other entry's glm-5.4 is not taken)
        assert!(rewrite_agents_md_model(src, &["glm-5.3".into(), "glm-5.4".into()]).is_none());
        // two entries, two different successors: ambiguous
        assert!(rewrite_agents_md_model(src, &["glm-5.4".into(), "glm-6".into()]).is_none());
        // two entries agreeing on one successor
        let (_, _, new) =
            rewrite_agents_md_model(src, &["glm-5.4".into(), "GLM-5.4".into()]).unwrap();
        assert_eq!(new, "GLM-5.4");
    }

    // (0.2.5, F02-4) Protected, on-request and inactive entries are not followed.
    #[test]
    fn roster_models_holds_only_entries_agents_md_may_follow() {
        let roster = c3_core::roster::validate_roster(
            "R.json",
            r#"{"roster_version":1,"reviewers":[
                {"provider":"zai","model":"glm-5.4"},
                {"provider":"astra","model":"glm-6","ext":{"reserve_percent":15}},
                {"provider":"wonder","model":"glm-7","ext":{"on_request":true}},
                {"provider":"off","model":"glm-8"}
            ],"ext":{"c3":{"inactive":["off"]}}}"#,
            None,
        );
        assert!(roster.error.is_empty(), "{}", roster.error);
        assert_eq!(roster_models(&roster), vec!["glm-5.4".to_string()]);
    }

    // (0.2.5, F02-1) A file with a second hard link (here: outside the repository) is never edited:
    // the external name keeps its bytes and the refusal is reported.
    #[test]
    fn a_hard_linked_agent_file_is_reported_and_never_edited() {
        let (base, repo) = repo_with_agents("hardlink");
        let outside = base.join("sentinel.md");
        std::fs::write(&outside, PINNED).unwrap();
        let inside = repo.join(".claude").join("agents").join("w.md");
        std::fs::hard_link(&outside, &inside).unwrap();
        let r = refresh_repo_models_at(&repo, &[]);
        assert!(r.fixes.is_empty(), "{r:?}");
        assert_eq!(r.problems.len(), 1, "{r:?}");
        assert!(r.problems[0].why.contains("2 hard links"), "{r:?}");
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), PINNED);
        assert_eq!(std::fs::read_to_string(&inside).unwrap(), PINNED);
        let line = hook_line_for(&r).unwrap();
        assert!(
            line.starts_with("c3: model settings NOT refreshed in this repository - .claude/agents/w.md claude-opus-4-8 -> opus: it has 2 hard links"),
            "{line}"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    // (0.2.5, F02-1) A linked `.claude/agents` (a junction on Windows, a symlink elsewhere) is not
    // edited; the file outside keeps its bytes and the refusal is reported.
    #[test]
    fn a_linked_agents_directory_is_reported_and_never_edited() {
        let base = scratch("linkdir");
        let repo = base.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join(".claude")).unwrap();
        let outside = base.join("outside-agents");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("w.md"), PINNED).unwrap();
        let link = repo.join(".claude").join("agents");
        #[cfg(windows)]
        {
            let ok = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(&link)
                .arg(&outside)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            assert!(ok, "mklink /J failed");
        }
        #[cfg(not(windows))]
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let r = refresh_repo_models_at(&repo, &[]);
        assert!(r.fixes.is_empty(), "{r:?}");
        assert_eq!(r.problems.len(), 1, "{r:?}");
        assert!(
            r.problems[0].why.contains(".claude/agents is a link"),
            "{r:?}"
        );
        assert_eq!(
            std::fs::read_to_string(outside.join("w.md")).unwrap(),
            PINNED
        );
        #[cfg(windows)]
        let _ = std::fs::remove_dir(&link);
        let _ = std::fs::remove_dir_all(&base);
    }

    // (0.2.5, F02-2) A concurrent edit between the read and the replace wins: the file keeps the
    // newer bytes, the repair is reported, and no temporary file is left behind.
    #[test]
    fn a_file_changed_after_the_read_is_left_as_it_is() {
        let (base, repo) = repo_with_agents("concurrent");
        let agents = repo.join(".claude").join("agents");
        let path = agents.join("w.md");
        let edited = "---\nname: w\nmodel: claude-opus-4-8\n---\nthe operator's new line\n";
        std::fs::write(&path, edited).unwrap();
        let edit = Edit {
            file: ".claude/agents/w.md".into(),
            root: &repo,
            snapshot: PINNED.as_bytes(),
            new_content: b"---\nname: w\nmodel: opus\n---\nbody\n",
        };
        let err = replace_file(&edit).unwrap_err();
        assert!(err.contains("changed while it was being repaired"), "{err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), edited);
        assert_eq!(names_in(&agents), vec!["w.md".to_string()]);
        let _ = std::fs::remove_dir_all(&base);
    }

    // (0.2.5, F02-2) A replace that fails (here the file is held open without delete sharing, so
    // the rename cannot replace it) leaves the original bytes - never a truncated file - and no
    // temporary file.
    #[cfg(windows)]
    #[test]
    fn a_failed_replace_leaves_the_original_bytes() {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x1;
        let (base, repo) = repo_with_agents("failreplace");
        let agents = repo.join(".claude").join("agents");
        let path = agents.join("w.md");
        std::fs::write(&path, PINNED).unwrap();
        let holder = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&path)
            .unwrap();
        let r = refresh_repo_models_at(&repo, &[]);
        drop(holder);
        assert!(r.fixes.is_empty(), "{r:?}");
        assert_eq!(r.problems.len(), 1, "{r:?}");
        assert!(
            r.problems[0].why.starts_with("could not replace it"),
            "{r:?}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), PINNED);
        assert_eq!(names_in(&agents), vec!["w.md".to_string()]);
        let _ = std::fs::remove_dir_all(&base);
    }

    // (0.2.5 D4, F13-9) THE REPAIR RACE: right before the rename (after the last check and the
    // re-read, `repair-replace`) `.claude/agents` - and, in a second round, its parent `.claude` - is
    // swapped for a link to an outside directory holding the same pinned file. The replacement never
    // lands outside (the outside file keeps its bytes, no temporary file appears there) and none is
    // left anywhere: on Windows the swap itself is refused (the agents directory's handle is held,
    // which pins it and every ancestor); elsewhere it happens and the repair completes in the
    // ORIGINAL directory - the one the handle names, now under its aside name.
    #[test]
    fn a_directory_swapped_for_a_link_before_the_replace_never_receives_the_repair() {
        for swap in [".claude/agents", ".claude"] {
            let (base, repo) = repo_with_agents("race-repair");
            let agents = repo.join(".claude").join("agents");
            std::fs::write(agents.join("w.md"), PINNED).unwrap();
            let outside = base.join("outside");
            let twin = if swap == ".claude" {
                outside.join("agents")
            } else {
                outside.clone()
            };
            std::fs::create_dir_all(&twin).unwrap();
            std::fs::write(twin.join("w.md"), PINNED).unwrap();
            let swapped_dir = repo.join(swap);
            let hook = c3_core::test_hooks::set_thread_hook(
                crate::contain::RACE_HOOK,
                &format!(
                    "repair-replace|{}|{}",
                    swapped_dir.display(),
                    outside.display()
                ),
            );
            let r = refresh_repo_models_at(&repo, &[]);
            drop(hook);
            let mut aside_name = swapped_dir.file_name().unwrap().to_os_string();
            aside_name.push(".c3-race-orig");
            let aside = swapped_dir.with_file_name(aside_name);
            let swapped = aside.exists();
            if cfg!(windows) {
                assert!(
                    !swapped,
                    "{swap}: a held directory and its ancestors cannot be renamed"
                );
            }
            assert_eq!(r.fixes.len(), 1, "{swap}: {r:?}");
            let original = match (swapped, swap) {
                (false, _) => agents.clone(),
                (true, ".claude") => aside.join("agents"),
                (true, _) => aside.clone(),
            };
            assert_eq!(
                std::fs::read_to_string(original.join("w.md")).unwrap(),
                "---\nname: w\nmodel: opus\n---\nbody\n",
                "{swap}: the repair is in the original directory"
            );
            assert_eq!(std::fs::read_to_string(twin.join("w.md")).unwrap(), PINNED);
            assert_eq!(names_in(&twin), vec!["w.md".to_string()], "{swap}");
            assert_eq!(names_in(&original), vec!["w.md".to_string()], "{swap}");
            if swapped {
                let _ = std::fs::remove_dir(&swapped_dir)
                    .or_else(|_| std::fs::remove_file(&swapped_dir));
            }
            assert_eq!(std::fs::read_to_string(twin.join("w.md")).unwrap(), PINNED);
            let _ = std::fs::remove_dir_all(&base);
        }
    }

    // (0.2.5, F02-1 / F02-2) The ordinary repair replaces the file by rename: the new bytes are in
    // place, no temporary file is left, and AGENTS.md follows its one successor.
    #[test]
    fn a_repair_replaces_the_file_and_leaves_no_temporary_file() {
        let (base, repo) = repo_with_agents("replace");
        let agents = repo.join(".claude").join("agents");
        std::fs::write(agents.join("w.md"), PINNED).unwrap();
        std::fs::write(
            repo.join("AGENTS.md"),
            "---\nmodel: GLM-5.3 # eck\n---\nx\n",
        )
        .unwrap();
        let r = refresh_repo_models_at(&repo, &["glm-5.4".to_string()]);
        assert!(r.problems.is_empty(), "{r:?}");
        assert_eq!(r.fixes.len(), 2, "{r:?}");
        assert_eq!(
            std::fs::read_to_string(agents.join("w.md")).unwrap(),
            "---\nname: w\nmodel: opus\n---\nbody\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("AGENTS.md")).unwrap(),
            "---\nmodel: GLM-5.4 # eck\n---\nx\n"
        );
        assert_eq!(names_in(&agents), vec!["w.md".to_string()]);
        assert!(!names_in(&repo).iter().any(|n| n.ends_with(".tmp")));
        let line = hook_line_for(&r).unwrap();
        assert_eq!(
            line,
            "c3: model settings refreshed in this repository - .claude/agents/w.md claude-opus-4-8 -> opus; AGENTS.md GLM-5.3 -> GLM-5.4 (left uncommitted)"
        );
        // a second pass has nothing to do
        assert_eq!(
            refresh_repo_models_at(&repo, &["glm-5.4".to_string()]),
            RepairReport::default()
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    // (0.2.5, F02-6) The root is found without git: the nearest ancestor holding `.git` (a
    // directory, or the file of a linked worktree).
    #[test]
    fn the_repo_root_is_found_by_walking_up_to_git() {
        let base = scratch("root");
        let repo = base.join("repo");
        let deep = repo.join("a").join("b");
        std::fs::create_dir_all(&deep).unwrap();
        // (an ancestor of the temp directory could hold a `.git` of its own: only `base` matters)
        assert!(find_repo_root(&deep).is_none_or(|r| !r.starts_with(&base)));
        std::fs::create_dir_all(repo.join(".git").join("worktrees").join("wt")).unwrap();
        std::fs::write(repo.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            repo.join(".git").join("worktrees").join("wt").join("HEAD"),
            "0123\n",
        )
        .unwrap();
        assert_eq!(find_repo_root(&deep), Some(repo.clone()));
        let wt = base.join("wt");
        std::fs::create_dir_all(wt.join("src")).unwrap();
        std::fs::write(wt.join(".git"), "gitdir: ../repo/.git/worktrees/wt\n").unwrap();
        assert_eq!(find_repo_root(&wt.join("src")), Some(wt.clone()));
        let _ = std::fs::remove_dir_all(&base);
    }

    // (R2, F13-2) A stray `.git` entry is not a repository: an empty or foreign file, a `gitdir:`
    // file whose target is missing or holds no HEAD, a directory without HEAD - each is the
    // nearest `.git`, so there is no root (the walk does not go on past it) and no repair.
    #[test]
    fn a_stray_git_entry_is_not_a_repository() {
        let base = scratch("stray");
        let dir = base.join("plain");
        let deep = dir.join("sub");
        std::fs::create_dir_all(&deep).unwrap();
        let dotgit = dir.join(".git");
        for body in [
            "",
            "not a repository\n",
            "gitdir:\n",
            "gitdir: ../missing\n",
            "x\ngitdir: ../plain\n",
        ] {
            std::fs::write(&dotgit, body).unwrap();
            assert_eq!(find_repo_root(&deep), None, "{body:?}");
        }
        // a gitdir target that exists but holds no HEAD, then one that does
        let target = base.join("admin");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(&dotgit, "gitdir: ../admin\n").unwrap();
        assert_eq!(find_repo_root(&deep), None);
        std::fs::write(target.join("HEAD"), "0123\n").unwrap();
        assert_eq!(find_repo_root(&deep), Some(dir.clone()));
        // an absolute target is read as such
        std::fs::write(&dotgit, format!("gitdir: {}\n", target.display())).unwrap();
        assert_eq!(find_repo_root(&deep), Some(dir.clone()));
        // a `.git` directory without HEAD is not a repository either
        std::fs::remove_file(&dotgit).unwrap();
        std::fs::create_dir_all(&dotgit).unwrap();
        assert_eq!(find_repo_root(&deep), None);
        std::fs::write(dotgit.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(find_repo_root(&deep), Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&base);
    }
}
