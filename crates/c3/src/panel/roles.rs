//! Role-file resolution (`Resolve-RoleFile`, `codex-consult-common.ps1:6418`).
//!
//! A panel member's `role` (and a single-run `-Role`) is a slug that names a role file: the
//! repository's `<collab-root>/roles/<name>.md` first, else the plugin's
//! `<plugin-root>/templates/role-<name>.md`. The file's trimmed text becomes the role paragraph
//! the prompt carries after the ask (`orchestrate` builds the line). C3 ships the built-in role
//! templates under `plugin/templates/role-*.md`; [`plugin_root`] resolves the plugin root at run
//! time (`CLAUDE_PLUGIN_ROOT`, else the parent of `CODEX_CONSULT_SCRIPTS_DIR`), so a shipped
//! template resolves with `source = "plugin"`. The "unknown role" error names both candidate paths.

use std::path::{Path, PathBuf};

/// A resolved role (`Resolve-RoleFile`'s record).
#[derive(Debug, Clone, Default)]
pub struct RoleInfo {
    pub name: String,
    pub path: String,
    /// `repository` | `plugin`.
    pub source: String,
    pub text: String,
    /// Non-empty when the role could not be resolved (a refusal message).
    pub error: String,
}

/// Whether a [`resolve_role_file`] error is a SAFETY refusal — a role file outside its roles
/// directory, a reparse-point role file, or a junctioned roles directory (wave 26b, D1) — as
/// opposed to a plain "unknown role" / "empty". C3 ships no plugin templates on disk, so a role
/// that exists only as a plugin template is "unknown" here; the caller tolerates that (no role
/// paragraph) but always refuses a safety problem.
pub fn is_role_refusal(error: &str) -> bool {
    error.starts_with("role file refused:")
}

/// Whether `path` is a reparse point (a symbolic link or, on Windows, a directory junction),
/// read WITHOUT following it (`FILE_ATTRIBUTE_REPARSE_POINT` / `symlink_metadata`).
fn is_reparse_point(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        std::fs::symlink_metadata(path)
            .map(|m| m.file_attributes() & 0x400 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        std::fs::symlink_metadata(path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    }
}

/// `Get-RoleFileProblem` (`codex-consult-common.ps1`): why the role file at `path` may not be used
/// — it is outside its `root` roles directory, is not a regular file (a directory, or a symbolic
/// link / reparse point), or sits under a junctioned directory. `""` when there is no problem (a
/// non-existent file is not a problem here — it is reported as an unknown role by the caller).
pub fn role_file_problem(path: &Path, root: &Path) -> String {
    let root_full = root.to_string_lossy();
    let root_full = root_full.trim_end_matches(['\\', '/']);
    let full = path.to_string_lossy().to_string();
    let sep = std::path::MAIN_SEPARATOR;
    let prefix = format!("{root_full}{sep}");
    let inside = if cfg!(windows) {
        full.to_lowercase().starts_with(&prefix.to_lowercase())
    } else {
        full.starts_with(&prefix)
    };
    if !inside {
        return format!("'{}' is outside '{root_full}'", path.display());
    }
    let md = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        // Neither a file nor a directory here (nor a dangling link we can read): not a problem;
        // the caller reports an unknown role.
        Err(_) => return String::new(),
    };
    let reparse = is_reparse_point(path);
    if md.file_type().is_dir() {
        let link = if reparse { " link (reparse point)" } else { "" };
        return format!("'{full}' is a directory{link}, not a regular file");
    }
    if reparse {
        return format!("'{full}' is a symbolic link or another reparse point, not a regular file");
    }
    // Walk up from the file's directory to the root: any junctioned directory on the way refuses.
    let mut dir = path.parent();
    while let Some(d) = dir {
        if is_reparse_point(d) {
            return format!(
                "the directory '{}' is a junction or symbolic link (reparse point)",
                d.display()
            );
        }
        let d_str = d.to_string_lossy();
        let d_trim = d_str.trim_end_matches(['\\', '/']);
        let same = if cfg!(windows) {
            d_trim.eq_ignore_ascii_case(root_full)
        } else {
            d_trim == root_full
        };
        if same {
            break;
        }
        dir = d.parent();
    }
    String::new()
}

/// The plugin root whose `templates/role-<name>.md` files ship the built-in roles (edge-cases,
/// security, tests, docs). Resolved the way the plugin's own root is known at run time:
/// `CLAUDE_PLUGIN_ROOT` (Claude Code sets it for a plugin's hooks and skills) if set, else the
/// parent of `CODEX_CONSULT_SCRIPTS_DIR` (what the harness points at the shim copy), else empty.
/// C3 ships the templates under `plugin/templates/` in the repository.
pub fn plugin_root() -> String {
    if let Ok(v) = std::env::var("CLAUDE_PLUGIN_ROOT") {
        if !v.trim().is_empty() {
            return v;
        }
    }
    if let Ok(v) = std::env::var("CODEX_CONSULT_SCRIPTS_DIR") {
        if !v.trim().is_empty() {
            if let Some(parent) = Path::new(v.trim()).parent() {
                return parent.to_string_lossy().to_string();
            }
        }
    }
    String::new()
}

/// A role name is a slug: lowercase letters, digits, dot, dash, underscore.
fn is_role_slug(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_'))
}

/// `Resolve-RoleFile`: resolve `name` against the repository roles dir then the plugin templates
/// dir. Either root may be empty (skipped). Returns the trimmed role text, or an error.
pub fn resolve_role_file(name: &str, collab_root: &Path, plugin_root: &str) -> RoleInfo {
    let mut r = RoleInfo {
        name: name.to_string(),
        ..Default::default()
    };
    if !is_role_slug(name) {
        r.error = format!(
            "role '{name}' is not a slug (lowercase letters, digits, dot, dash, underscore)"
        );
        return r;
    }
    // (candidate path, source, roles-dir).
    let mut candidates: Vec<(PathBuf, &'static str, PathBuf)> = Vec::new();
    let repo_roles = collab_root.join("roles");
    candidates.push((
        repo_roles.join(format!("{name}.md")),
        "repository",
        repo_roles.clone(),
    ));
    if !plugin_root.is_empty() {
        let tmpl = Path::new(plugin_root).join("templates");
        candidates.push((tmpl.join(format!("role-{name}.md")), "plugin", tmpl));
    }
    for (path, source, root) in &candidates {
        // (wave 26b, D1) a roles directory that is itself a junction refuses every role of it.
        if root.exists() && is_reparse_point(root) {
            r.error = format!(
                "role file refused: the {source} roles directory '{}' is a junction or symbolic link (reparse point)",
                root.display()
            );
            return r;
        }
        let why = role_file_problem(path, root);
        if !why.is_empty() {
            r.error = format!("role file refused: {why}");
            return r;
        }
        if path.is_file() {
            let text = std::fs::read_to_string(path).unwrap_or_default();
            let text = text.trim().to_string();
            if text.is_empty() {
                r.error = format!("the role file '{}' is empty", path.display());
                return r;
            }
            r.path = path.display().to_string();
            r.source = (*source).to_string();
            r.text = text;
            return r;
        }
    }
    // Unknown: name the candidate paths and the known roles.
    let mut known: Vec<String> = Vec::new();
    for (path, source, _root) in &candidates {
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for entry in rd.flatten() {
                let file_name = entry.file_name().to_string_lossy().to_string();
                if !file_name.ends_with(".md") {
                    continue;
                }
                let mut base = file_name.trim_end_matches(".md").to_string();
                if *source == "plugin" {
                    if !base.starts_with("role-") {
                        continue;
                    }
                    base = base["role-".len()..].to_string();
                }
                if is_role_slug(&base) && !known.contains(&base) {
                    known.push(base);
                }
            }
        }
    }
    known.sort();
    let repo_hint = format!("{} nor ", repo_roles.join(format!("{name}.md")).display());
    r.error = format!(
        "unknown role '{name}' (no {repo_hint}templates/role-{name}.md; known: {})",
        if known.is_empty() {
            "none".to_string()
        } else {
            known.join(", ")
        }
    );
    r
}

/// The role paragraph the prompt carries (`codex-consult.ps1:3558`): the header sentence, a
/// CRLF, then the role text with its own newlines normalised to CRLF. Empty for no role.
pub fn role_prompt_line(role: &RoleInfo) -> String {
    if role.name.is_empty() || role.text.is_empty() {
        return String::new();
    }
    let body = role.text.replace("\r\n", "\n").replace('\n', "\r\n");
    format!(
        "Your role in this review: {}. Focus on what it asks for; the reply format, the verdict rules and the read-only rule stay as stated.\r\n{}",
        role.name, body
    )
}

// ------------------------------------------------------------------------- role assignment (D8)

/// One seated member fed to [`select_role_assignment`]: its roster position, its routing score
/// (`RoutingScore.score`; the neutral prior when unrated) and the role slugs it is willing to
/// take (`entry.roles`).
#[derive(Debug, Clone)]
pub struct RoleMember {
    pub position: i64,
    pub score: f64,
    pub roles: Vec<String>,
}

/// `Select-RoleAssignment`'s result (`codex-consult-common.ps1:6490`).
#[derive(Debug, Clone, Default)]
pub struct RoleAssignment {
    /// seat position -> role slug.
    pub of: std::collections::HashMap<i64, String>,
    /// The greedy-fallback note (empty when every willingness was honoured).
    pub note: String,
    /// A refusal (more roles than members); the panel refuses before anything starts.
    pub error: String,
}

/// Kuhn's augmenting-path step (`Find-RoleAugment`): try to match role `role` to a member.
fn find_role_augment(
    role: usize,
    willing: &[Vec<usize>],
    match_of: &mut std::collections::HashMap<usize, usize>,
    seen: &mut std::collections::HashSet<usize>,
) -> bool {
    for &m in &willing[role] {
        if seen.contains(&m) {
            continue;
        }
        seen.insert(m);
        let free = !match_of.contains_key(&m);
        if free || find_role_augment(match_of[&m], willing, match_of, seen) {
            match_of.insert(m, role);
            return true;
        }
    }
    false
}

/// `Test-RoleMatching`: can every constrained role be matched to a distinct free member?
/// `role_idx` are indices into `willing`; `free` restricts the members considered.
fn test_role_matching(
    role_idx: &[usize],
    free: &[usize],
    willing_of: &std::collections::HashMap<usize, Vec<usize>>,
) -> bool {
    let willing: Vec<Vec<usize>> = role_idx
        .iter()
        .map(|ri| {
            willing_of
                .get(ri)
                .map(|w| w.iter().copied().filter(|m| free.contains(m)).collect())
                .unwrap_or_default()
        })
        .collect();
    let mut match_of: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for i in 0..willing.len() {
        let mut seen = std::collections::HashSet::new();
        if !find_role_augment(i, &willing, &mut match_of, &mut seen) {
            return false;
        }
    }
    true
}

/// `Select-RoleAssignment` (wave 26/26b, D8): assign the panel's `-Roles` to its seated members
/// by score rank and willingness. The lexicographically best assignment that honours every
/// willingness when one exists (Kuhn's matching decides feasibility); else a greedy fallback with
/// a note. Ranking is score descending, then seat order ascending (stable).
pub fn select_role_assignment(members: &[RoleMember], roles: &[String]) -> RoleAssignment {
    let mut r = RoleAssignment::default();
    let list: Vec<&RoleMember> = members.iter().collect();
    if roles.len() > list.len() {
        r.error = format!(
            "-Roles names {} roles for {} panel member{} (at most one role each)",
            roles.len(),
            list.len(),
            if list.len() != 1 { "s" } else { "" }
        );
        return r;
    }
    // Rank: score desc, seat asc (stable). `order[i]` is the member at rank i.
    let mut ranked: Vec<(usize, &RoleMember)> = list.iter().copied().enumerate().collect();
    ranked.sort_by(|a, b| {
        b.1.score
            .partial_cmp(&a.1.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    let order: Vec<&RoleMember> = ranked.iter().map(|(_, m)| *m).collect();

    // willingOf[role] = rank indices of members willing to take it; constrained = has any.
    let mut willing_of: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    let mut constrained: Vec<usize> = Vec::new();
    for (ri, role) in roles.iter().enumerate() {
        let w: Vec<usize> = order
            .iter()
            .enumerate()
            .filter(|(_, m)| m.roles.iter().any(|x| x == role))
            .map(|(mi, _)| mi)
            .collect();
        if !w.is_empty() {
            constrained.push(ri);
        }
        willing_of.insert(ri, w);
    }
    let all_idx: Vec<usize> = (0..order.len()).collect();

    if test_role_matching(&constrained, &all_idx, &willing_of) {
        let mut used: Vec<usize> = Vec::new();
        #[allow(clippy::needless_range_loop)]
        for (ri, role) in roles.iter().enumerate() {
            let later: Vec<usize> = constrained.iter().copied().filter(|&c| c > ri).collect();
            for mi in 0..order.len() {
                if used.contains(&mi) {
                    continue;
                }
                if constrained.contains(&ri) && !willing_of[&ri].contains(&mi) {
                    continue;
                }
                let free: Vec<usize> = all_idx
                    .iter()
                    .copied()
                    .filter(|&x| x != mi && !used.contains(&x))
                    .collect();
                if test_role_matching(&later, &free, &willing_of) {
                    used.push(mi);
                    r.of.insert(order[mi].position, role.clone());
                    break;
                }
            }
        }
        return r;
    }

    // Greedy fallback: a note, then each role to the first willing member (by rank), else first left.
    let unmet: Vec<String> = constrained
        .iter()
        .map(|&ri| {
            let ws: Vec<String> = willing_of[&ri]
                .iter()
                .map(|&mi| format!("#{}", order[mi].position))
                .collect();
            format!("{}: willing {}", roles[ri], ws.join(" "))
        })
        .collect();
    r.note = format!(
        "roles: no assignment gives every role a willing member ({}) - the roles went by score rank, a willing member first where one was left",
        unmet.join("; ")
    );
    let mut left: Vec<usize> = (0..order.len()).collect();
    for role in roles {
        let willing = left
            .iter()
            .position(|&mi| order[mi].roles.iter().any(|x| x == role));
        let take = match willing {
            Some(p) => Some(p),
            None if left.is_empty() => None,
            None => Some(0),
        };
        if let Some(pos) = take {
            let mi = left.remove(pos);
            r.of.insert(order[mi].position, role.clone());
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "c3-roles-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(d.join("roles")).unwrap();
        d
    }

    #[test]
    fn resolves_a_repository_role() {
        let root = scratch();
        std::fs::write(
            root.join("roles").join("adversary.md"),
            "Attack the design.\n",
        )
        .unwrap();
        let r = resolve_role_file("adversary", &root, "");
        assert_eq!(r.error, "");
        assert_eq!(r.source, "repository");
        assert_eq!(r.text, "Attack the design.");
        let line = role_prompt_line(&r);
        assert!(line.starts_with("Your role in this review: adversary. Focus on what it asks for;"));
        assert!(line.ends_with("Attack the design."));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn resolves_a_plugin_template_with_source_plugin() {
        // A role that exists only as a shipped plugin template resolves from <plugin_root>/templates
        // with source "plugin" (ROLEFILE D1).
        let root = scratch();
        let plugin = root.join("plugin");
        std::fs::create_dir_all(plugin.join("templates")).unwrap();
        std::fs::write(
            plugin.join("templates").join("role-security.md"),
            "Hunt for security holes.\n",
        )
        .unwrap();
        let r = resolve_role_file("security", &root, &plugin.to_string_lossy());
        assert_eq!(r.error, "");
        assert_eq!(r.source, "plugin");
        assert_eq!(r.text, "Hunt for security holes.");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_a_non_slug() {
        let root = scratch();
        let r = resolve_role_file("Bad Role", &root, "");
        assert!(r.error.contains("is not a slug"));
        let _ = std::fs::remove_dir_all(root);
    }

    fn rm(position: i64, score: f64, roles: &[&str]) -> RoleMember {
        RoleMember {
            position,
            score,
            roles: roles.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn more_roles_than_members_refuses() {
        let members = vec![rm(1, 1.5, &[])];
        let a = select_role_assignment(&members, &["adversary".into(), "scribe".into()]);
        assert!(a.error.contains("names 2 roles for 1 panel member"));
    }

    #[test]
    fn willingness_is_honoured_when_a_matching_exists() {
        // #2 is willing for adversary; the assignment must give it that role even though #1 ranks
        // higher (a feasible matching honours every willingness).
        let members = vec![rm(1, 2.0, &[]), rm(2, 1.0, &["adversary"])];
        let a = select_role_assignment(&members, &["adversary".into()]);
        assert!(a.error.is_empty());
        assert!(a.note.is_empty());
        assert_eq!(a.of.get(&2).map(String::as_str), Some("adversary"));
        assert!(!a.of.contains_key(&1));
    }

    #[test]
    fn no_willing_member_falls_back_by_rank_with_a_note() {
        // Nobody is willing for either role → greedy by score rank, with the note.
        let members = vec![rm(1, 2.0, &[]), rm(2, 1.0, &[])];
        let a = select_role_assignment(&members, &["adversary".into(), "scribe".into()]);
        // Not constrained (no willing) → both roles matchable (vacuously), assigned by rank.
        assert!(a.error.is_empty());
        assert_eq!(a.of.get(&1).map(String::as_str), Some("adversary"));
        assert_eq!(a.of.get(&2).map(String::as_str), Some("scribe"));
    }

    #[test]
    fn conflicting_willingness_notes_the_fallback() {
        // Two roles, both only #1 willing → no assignment honours both; greedy + note.
        let members = vec![rm(1, 2.0, &["adversary", "scribe"]), rm(2, 1.0, &[])];
        let a = select_role_assignment(&members, &["adversary".into(), "scribe".into()]);
        assert!(a
            .note
            .contains("no assignment gives every role a willing member"));
        // #1 (willing, top rank) takes adversary; scribe falls to #2.
        assert_eq!(a.of.get(&1).map(String::as_str), Some("adversary"));
        assert_eq!(a.of.get(&2).map(String::as_str), Some("scribe"));
    }

    #[test]
    fn unknown_role_names_candidates_and_known() {
        let root = scratch();
        std::fs::write(root.join("roles").join("scribe.md"), "notes").unwrap();
        let r = resolve_role_file("ghost", &root, "");
        assert!(r.error.contains("unknown role 'ghost'"));
        assert!(r.error.contains("known: scribe"));
        let _ = std::fs::remove_dir_all(root);
    }
}
