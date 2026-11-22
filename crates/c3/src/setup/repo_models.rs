//! Repair a repository's model settings in place (follow-up to `c3 doctor`): C3 rewrites a
//! stale agent model pin to the floating alias, and lets `AGENTS.md` (the OpenCode agent file
//! eckSnapshot writes) follow the roster's successor. It makes exactly two kinds of edit, each
//! changing ONE value of a file and nothing else (every other byte - the quotes around the value,
//! a trailing comment, the line endings, a BOM - is preserved). Nothing here runs git or any other
//! process, commits, or writes outside the repository root; the edits are left uncommitted for
//! `c3 finish`.
//!
//! (0.2.5, F02-1 / F02-2) A file is never written through: the new content goes to a sibling
//! temporary file in the same directory, which is read back, the original is re-read and compared
//! with the snapshot the edit was computed from (a concurrent edit aborts the repair and leaves the
//! file as it is), and the temporary file then REPLACES the original by rename - so a failed or
//! interrupted repair never leaves a truncated file, and a second name of the file (a hard link)
//! never sees the edit. A file with more than one hard link, a link (symlink, junction) and a file
//! under a linked `.claude` / `.claude/agents` are not edited at all; the links are checked again
//! immediately before the rename. Every repair that was not made is reported with its reason.
//!
//! It is used from two places: `c3 doctor --fix` (which then re-reports what is left) and the
//! `c3 hook` SessionStart line (automatic, default ON, off with `C3_REPO_MODELS=off` or
//! `"repo_models": false` in `<codex home>/c3/config.json`). Inside a child C3 started
//! (`C3_CHILD`) it does nothing at all.

use std::io::Write;
use std::path::{Path, PathBuf};

use c3_core::claude::model_family;
use c3_core::roster::Roster;

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
/// * the value is rewritten only when exactly ONE successor id results; two entries carrying
///   different successors leave it alone (ambiguous).
///
/// The new value keeps the casing of the original prefix/suffix around the successor's version
/// (`GLM-5.3` -> `GLM-5.4`, `Qwen3.8-Max` -> `Qwen4-Max`). A Claude-family value is the agent-file
/// rule's, not this one. Returns `(new_content, old, new)`.
pub fn rewrite_agents_md_model(
    content: &str,
    roster_models: &[String],
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
            if !succs.contains(&s) {
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
/// AGENTS.md rule). Inside a child C3 started (`C3_CHILD`, F04-4) it does nothing and says so
/// ([`RepairReport::skipped_in_child`]). Otherwise every file is replaced, never written through
/// (see the module doc); a repair that cannot be made safely is reported, never forced. Nothing is
/// written outside `repo_root` and no process is started.
pub fn refresh_repo_models(repo_root: &Path, roster_models: &[String]) -> RepairReport {
    if crate::engines::in_c3_child() {
        return RepairReport {
            skipped_in_child: true,
            ..RepairReport::default()
        };
    }
    refresh_repo_models_at(repo_root, roster_models)
}

/// [`refresh_repo_models`] without the C3-child check.
fn refresh_repo_models_at(repo_root: &Path, roster_models: &[String]) -> RepairReport {
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
            let dirs = [
                (claude_dir.as_path(), ".claude"),
                (agents.as_path(), ".claude/agents"),
            ];
            let edit = Edit {
                file: format!(".claude/agents/{name}"),
                path: &path,
                dirs: &dirs,
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
            if let Some((new_content, old, new)) = rewrite_agents_md_model(content, roster_models) {
                let edit = Edit {
                    file: "AGENTS.md".to_string(),
                    path: &agents_md,
                    dirs: &[],
                    snapshot: &snapshot,
                    new_content: new_content.as_bytes(),
                };
                apply(&mut report, &edit, old, new);
            }
        }
    }

    report
}

/// One planned edit: the file, the directories under the repository root that must be real
/// directories for it, the bytes the edit was computed from and the new bytes.
pub(crate) struct Edit<'a> {
    pub(crate) file: String,
    pub(crate) path: &'a Path,
    pub(crate) dirs: &'a [(&'a Path, &'a str)],
    pub(crate) snapshot: &'a [u8],
    pub(crate) new_content: &'a [u8],
}

/// Make one edit (or record why not) in `report`.
fn apply(report: &mut RepairReport, edit: &Edit<'_>, old: String, new: String) {
    let result = match edit_refusal(edit.dirs, edit.path) {
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

/// Why `path` may not be edited, or `None` when it may: every directory of `dirs` must be a real
/// directory (not a symlink or junction), the file a regular file that is not a link, with exactly
/// one name (a second hard link would see the edit too).
pub(crate) fn edit_refusal(dirs: &[(&Path, &str)], path: &Path) -> Option<String> {
    for (dir, label) in dirs {
        if !is_real_dir(dir) {
            return Some(format!(
                "{label} is a link (a symlink or junction) or no directory, so the file may lie outside the repository - not edited"
            ));
        }
    }
    if !is_real_file(path) {
        return Some(
            "it is a link (a symlink or junction) or no regular file, so its target may lie outside the repository - not edited"
                .to_string(),
        );
    }
    match link_count(path) {
        Ok(1) => None,
        Ok(n) => Some(format!(
            "it has {n} hard links, so an edit would change the other names too - not edited"
        )),
        Err(e) => Some(format!("could not count its hard links ({e}) - not edited")),
    }
}

/// Replace `edit.path` with `edit.new_content` without writing through it: a sibling temporary
/// file is written, synced and read back; the links are checked again and the original re-read and
/// compared with `edit.snapshot`; then the temporary file is renamed over the original. On any
/// failure the temporary file is removed and the original is untouched.
pub(crate) fn replace_file(edit: &Edit<'_>) -> Result<(), String> {
    let dir = edit
        .path
        .parent()
        .ok_or_else(|| "it has no parent directory - not edited".to_string())?;
    let name = edit
        .path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(
        ".{name}.c3-repair-{}-{nanos}.tmp",
        std::process::id()
    ));
    let result = write_then_rename(edit, &tmp);
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// The body of [`replace_file`] (the caller removes `tmp` on an error).
fn write_then_rename(edit: &Edit<'_>, tmp: &Path) -> Result<(), String> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(tmp)
        .map_err(|e| format!("could not create the temporary file ({e}) - left as it was"))?;
    f.write_all(edit.new_content)
        .and_then(|()| f.sync_all())
        .map_err(|e| format!("could not write the temporary file ({e}) - left as it was"))?;
    drop(f);
    #[cfg(unix)]
    {
        // the replacement keeps the original's permission bits
        if let Ok(m) = std::fs::metadata(edit.path) {
            let _ = std::fs::set_permissions(tmp, m.permissions());
        }
    }
    match std::fs::read(tmp) {
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
    // Immediately before the replace: the links once more, then the content against the snapshot
    // the edit was computed from (an editor's save in between wins; the repair is dropped).
    if let Some(why) = edit_refusal(edit.dirs, edit.path) {
        return Err(why);
    }
    match std::fs::read(edit.path) {
        Ok(b) if b == edit.snapshot => {}
        Ok(_) => return Err("it changed while it was being repaired - left as it was".to_string()),
        Err(e) => return Err(format!("could not re-read it ({e}) - left as it was")),
    }
    std::fs::rename(tmp, edit.path)
        .map_err(|e| format!("could not replace it ({e}) - left as it was"))
}

/// Whether the metadata (read without following a link) is a link: a symlink, or on Windows any
/// reparse point (a junction included).
fn is_link(m: &std::fs::Metadata) -> bool {
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

/// A directory that is itself there, not a link to one (`symlink_metadata` does not follow a
/// symlink; a Windows junction is a reparse point).
fn is_real_dir(p: &Path) -> bool {
    std::fs::symlink_metadata(p)
        .map(|m| m.file_type().is_dir() && !is_link(&m))
        .unwrap_or(false)
}

/// A regular file that is itself there, not a link to one.
fn is_real_file(p: &Path) -> bool {
    std::fs::symlink_metadata(p)
        .map(|m| m.file_type().is_file() && !is_link(&m))
        .unwrap_or(false)
}

/// How many names (hard links) the file at `p` has, read without following a link.
#[cfg(unix)]
fn link_count(p: &Path) -> std::io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(std::fs::symlink_metadata(p)?.nlink())
}

/// How many names (hard links) the file at `p` has (`GetFileInformationByHandle`'s
/// `nNumberOfLinks`, the file opened without following a reparse point).
#[cfg(windows)]
fn link_count(p: &Path) -> std::io::Result<u64> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    #[allow(non_snake_case)]
    #[repr(C)]
    struct ByHandleFileInformation {
        dwFileAttributes: u32,
        ftCreationTime: [u32; 2],
        ftLastAccessTime: [u32; 2],
        ftLastWriteTime: [u32; 2],
        dwVolumeSerialNumber: u32,
        nFileSizeHigh: u32,
        nFileSizeLow: u32,
        nNumberOfLinks: u32,
        nFileIndexHigh: u32,
        nFileIndexLow: u32,
    }
    extern "system" {
        fn GetFileInformationByHandle(
            file: *mut std::ffi::c_void,
            info: *mut ByHandleFileInformation,
        ) -> i32;
    }
    let f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(p)?;
    // SAFETY: an all-zero BY_HANDLE_FILE_INFORMATION is a valid value of this plain C struct.
    let mut info: ByHandleFileInformation = unsafe { std::mem::zeroed() };
    // SAFETY: `f` is an open file handle for the duration of the call and `info` points at a
    // writable struct of the layout the function fills.
    let ok = unsafe { GetFileInformationByHandle(f.as_raw_handle(), &mut info) };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(u64::from(info.nNumberOfLinks))
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
/// upwards, holding a `.git` entry - a directory, or the `.git` FILE of a linked worktree or a
/// submodule. `None` when no ancestor has one.
pub fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        if std::fs::symlink_metadata(d.join(".git")).is_ok() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
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
/// read-only, and a worker's edit would land in its patch and could break its scope check.
pub fn hook_line() -> Option<String> {
    if !enabled() || crate::engines::in_c3_child() {
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

    /// A scratch repository `<base>/repo` with `.git` and `.claude/agents/`.
    fn repo_with_agents(tag: &str) -> (PathBuf, PathBuf) {
        let base = scratch(tag);
        let repo = base.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
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
        let claude = repo.join(".claude");
        let dirs = [
            (claude.as_path(), ".claude"),
            (agents.as_path(), ".claude/agents"),
        ];
        let edit = Edit {
            file: ".claude/agents/w.md".into(),
            path: &path,
            dirs: &dirs,
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
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        assert_eq!(find_repo_root(&deep), Some(repo.clone()));
        let wt = base.join("wt");
        std::fs::create_dir_all(wt.join("src")).unwrap();
        std::fs::write(wt.join(".git"), "gitdir: ../repo/.git/worktrees/wt\n").unwrap();
        assert_eq!(find_repo_root(&wt.join("src")), Some(wt.clone()));
        let _ = std::fs::remove_dir_all(&base);
    }
}
