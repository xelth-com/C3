//! Path normalisation (`Get-RepoRelativePath`): repo-relative POSIX paths.

use std::path::Path;

/// The `/`-separated path of `path` relative to `root`; `Some("")` when they are the
/// same directory; `None` when `path` is not under `root`. Case-insensitive on
/// Windows - every letter folded, as the plugin's `OrdinalIgnoreCase`. Mirrors
/// `Get-RepoRelativePath`. (Wave 7, F36-1) The root is stripped char by char, never by
/// slicing the candidate at the root's byte length: a candidate that does not start with
/// the root - `C:\日本\.claude` against `C:\repo` - is `None`, not a panic.
pub fn repo_relative(root: &Path, path: &Path) -> Option<String> {
    let full = normalize(path);
    let root_full = normalize(root);
    let ci = cfg!(windows);
    if eq(&full, &root_full, ci) {
        return Some(String::new());
    }
    for sep in ['\\', '/'] {
        let prefix = format!("{root_full}{sep}");
        if let Some(rest) = strip_prefix(&full, &prefix, ci) {
            return Some(rest.replace('\\', "/"));
        }
    }
    None
}

fn normalize(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    s.trim_end_matches(['\\', '/']).to_string()
}

fn eq(a: &str, b: &str, ci: bool) -> bool {
    if ci {
        a.chars().map(fold).eq(b.chars().map(fold))
    } else {
        a == b
    }
}

/// `hay` without its leading `prefix`, `None` when it does not start with it - compared char
/// by char (case folded when `ci`), so the rest always begins at a char boundary of `hay`
/// (wave 7, F36-1: the byte length of `prefix` is never an index into `hay`).
fn strip_prefix<'a>(hay: &'a str, prefix: &str, ci: bool) -> Option<&'a str> {
    if !ci {
        return hay.strip_prefix(prefix);
    }
    let mut rest = hay.chars();
    for p in prefix.chars() {
        if fold(rest.next()?) != fold(p) {
            return None;
        }
    }
    Some(rest.as_str())
}

/// The case fold of `OrdinalIgnoreCase`: a char's simple uppercase mapping (the char itself
/// when its uppercase is not a single char, as `ß`).
fn fold(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_uppercase();
    }
    let mut up = c.to_uppercase();
    match (up.next(), up.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (Wave 7, F36-1, RC1) A candidate whose byte at the root prefix's length lies inside a
    /// multi-byte char is outside the root - returned, never a panic (the root `/repo/` is six
    /// bytes, byte six of `/日本/.claude` is inside `本`).
    #[test]
    fn a_unicode_candidate_outside_the_root_is_not_sliced() {
        assert_eq!(
            repo_relative(Path::new("/repo"), Path::new("/日本/.claude")),
            None
        );
        assert_eq!(repo_relative(Path::new("/re"), Path::new("/é/x")), None);
        // a shorter candidate, and one that ends inside the prefix
        assert_eq!(repo_relative(Path::new("/repo"), Path::new("/日")), None);
        assert_eq!(repo_relative(Path::new("/日本"), Path::new("/日")), None);
        // inside stays inside, the rest from a char boundary
        assert_eq!(
            repo_relative(Path::new("/日本"), Path::new("/日本/.claude/é")),
            Some(".claude/é".to_string())
        );
        assert_eq!(
            repo_relative(Path::new("/日本/"), Path::new("/日本")),
            Some(String::new())
        );
        assert_eq!(strip_prefix("/日本/x", "/repo/", true), None);
        assert_eq!(strip_prefix("/日本/x", "/repo/", false), None);
        assert_eq!(strip_prefix("/日本/x", "/日本/", true), Some("x"));
        assert_eq!(strip_prefix("/日本/x", "/日本/", false), Some("x"));
    }

    /// (F36-1, RC1) The reviewer's Windows case: root `C:\repo` (prefix eight bytes), candidate
    /// `C:\日本\.claude` (byte eight inside `本`); every letter folded as `OrdinalIgnoreCase`.
    #[cfg(windows)]
    #[test]
    fn a_unicode_config_dir_outside_a_drive_root_is_not_sliced() {
        assert_eq!(
            repo_relative(Path::new(r"C:\repo"), Path::new(r"C:\日本\.claude")),
            None
        );
        assert_eq!(
            repo_relative(Path::new(r"C:\日本"), Path::new(r"c:\日本\.claude")),
            Some(".claude".to_string())
        );
        assert_eq!(
            repo_relative(
                Path::new(r"C:\scratch\CAFÉ"),
                Path::new(r"C:\scratch\café\x")
            ),
            Some("x".to_string())
        );
        assert_eq!(
            repo_relative(Path::new(r"C:\Straße"), Path::new(r"C:\STRASSE\x")),
            None
        );
    }
}
