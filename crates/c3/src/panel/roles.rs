//! Role-file resolution (`Resolve-RoleFile`, `codex-consult-common.ps1:6418`).
//!
//! A panel member's `role` (and a single-run `-Role`) is a slug that names a role file: the
//! repository's `<collab-root>/roles/<name>.md` first, else the plugin's
//! `<plugin-root>/templates/role-<name>.md`. The file's trimmed text becomes the role paragraph
//! the prompt carries after the ask (`orchestrate` builds the line). C3 ships no plugin
//! templates on disk, so `plugin_root` is normally empty and only repository roles resolve;
//! the "unknown role" error still names both candidate paths as the plugin does.

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
    for (path, source, _root) in &candidates {
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
    fn rejects_a_non_slug() {
        let root = scratch();
        let r = resolve_role_file("Bad Role", &root, "");
        assert!(r.error.contains("is not a slug"));
        let _ = std::fs::remove_dir_all(root);
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
