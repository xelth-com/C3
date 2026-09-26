//! The task-slug newtype and path containment (`-Task`/`-ReplyName` validation and the
//! artifact/handoff path rule).
//!
//! The plugin gates every task id and reply name through one regex
//! (`codex-consult.ps1:1009-1012`): `^[A-Za-z0-9][A-Za-z0-9._-]*$` - a leading letter or
//! digit, then letters, digits, dot, dash and underscore, no lowercasing. C3 makes that a
//! [`TaskSlug`] newtype so a `<task>` can never be an empty string, a path fragment
//! (`../escape`, `C:\abs`) or a name with a separator in it: the store joins the slug onto
//! `.collab/` for every file it touches, and an unchecked join with an absolute or
//! parent-relative string escapes the collaboration root (F02-15, F08-9, F04-12).
//!
//! [`contained_join`] is the second half: a caller-supplied *relative* path (a handoff
//! file, an artifact) is joined onto a base only after it is proven relative, free of `..`
//! and of a root/prefix component, so a per-consultation write stays inside the task
//! directory (F02-15, F03-10, F04-12).

use std::path::{Component, Path, PathBuf};

/// A validated task id / reply-name slug: `^[A-Za-z0-9][A-Za-z0-9._-]*$`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskSlug(String);

/// Why a string is not a valid [`TaskSlug`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlugError {
    /// What was rejected (the offending string, for the message).
    pub value: String,
}

impl std::fmt::Display for SlugError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The plugin's own wording (codex-consult.ps1:1011), generalised to the value.
        write!(
            f,
            "'{}' is not a slug (letters, digits, dot, dash, underscore; the first character a letter or digit)",
            self.value
        )
    }
}

impl std::error::Error for SlugError {}

impl TaskSlug {
    /// Validate `s` against the plugin's slug rule (`^[A-Za-z0-9][A-Za-z0-9._-]*$`).
    pub fn new(s: impl Into<String>) -> Result<TaskSlug, SlugError> {
        let s = s.into();
        if is_slug(&s) {
            Ok(TaskSlug(s))
        } else {
            Err(SlugError { value: s })
        }
    }

    /// The slug text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TaskSlug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for TaskSlug {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// The plugin slug predicate: a leading `[A-Za-z0-9]`, then `[A-Za-z0-9._-]*`.
pub fn is_slug(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Why a relative path could not be safely joined onto a base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    /// The path is absolute (or has a root/prefix), so joining it would discard the base.
    NotRelative(String),
    /// The path contains a `..` component, so it could escape the base.
    ParentEscape(String),
    /// The path contains a bare `.` root only, or is empty.
    Empty(String),
}

impl std::fmt::Display for PathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathError::NotRelative(p) => {
                write!(
                    f,
                    "path '{p}' must be relative to the task directory (it is absolute or rooted)"
                )
            }
            PathError::ParentEscape(p) => {
                write!(f, "path '{p}' must not contain a '..' component (it could escape the task directory)")
            }
            PathError::Empty(p) => write!(f, "path '{p}' is empty"),
        }
    }
}

impl std::error::Error for PathError {}

/// Join `rel` onto `base`, but only after proving `rel` is a contained relative path:
/// not absolute, not rooted/prefixed, and free of any `..` component. The returned path is
/// guaranteed to be under `base`. Mirrors the containment the plugin's store paths assume
/// but never checked (F02-15, F03-10, F04-12).
pub fn contained_join(base: &Path, rel: &str) -> Result<PathBuf, PathError> {
    if rel.is_empty() {
        return Err(PathError::Empty(rel.to_string()));
    }
    let rp = Path::new(rel);
    let mut out = base.to_path_buf();
    let mut pushed = false;
    for comp in rp.components() {
        match comp {
            Component::Normal(part) => {
                out.push(part);
                pushed = true;
            }
            Component::CurDir => {}
            Component::ParentDir => return Err(PathError::ParentEscape(rel.to_string())),
            Component::RootDir | Component::Prefix(_) => {
                return Err(PathError::NotRelative(rel.to_string()))
            }
        }
    }
    if !pushed {
        return Err(PathError::Empty(rel.to_string()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_accepts_plugin_shapes() {
        assert!(TaskSlug::new("c3-core-contract").is_ok());
        assert!(TaskSlug::new("Wave24b").is_ok());
        assert!(TaskSlug::new("a.b_c-1").is_ok());
        assert!(TaskSlug::new("9lives").is_ok());
    }

    #[test]
    fn slug_rejects_separators_and_traversal() {
        assert!(TaskSlug::new("").is_err());
        assert!(TaskSlug::new(".hidden").is_err()); // leading dot
        assert!(TaskSlug::new("-lead").is_err()); // leading dash
        assert!(TaskSlug::new("a/b").is_err());
        assert!(TaskSlug::new("a\\b").is_err());
        assert!(TaskSlug::new("..").is_err());
        assert!(TaskSlug::new("C:").is_err());
        assert!(TaskSlug::new("a b").is_err());
    }

    #[test]
    fn contained_join_stays_under_base() {
        let base = Path::new("/collab/task");
        let p = contained_join(base, "handoffs/15-x.md").unwrap();
        assert!(p.ends_with("handoffs/15-x.md"));
        assert!(p.starts_with(base));
    }

    #[test]
    fn contained_join_rejects_escapes() {
        let base = Path::new("/collab/task");
        assert_eq!(
            contained_join(base, "../evil"),
            Err(PathError::ParentEscape("../evil".into()))
        );
        assert!(matches!(
            contained_join(base, "/etc/passwd"),
            Err(PathError::NotRelative(_))
        ));
        assert_eq!(contained_join(base, ""), Err(PathError::Empty("".into())));
    }
}
