//! Path normalisation (`Get-RepoRelativePath`): repo-relative POSIX paths.

use std::path::Path;

/// The `/`-separated path of `path` relative to `root`; `Some("")` when they are the
/// same directory; `None` when `path` is not under `root`. Case-insensitive on
/// Windows. Mirrors `Get-RepoRelativePath`.
pub fn repo_relative(root: &Path, path: &Path) -> Option<String> {
    let full = normalize(path);
    let root_full = normalize(root);
    let ci = cfg!(windows);
    if eq(&full, &root_full, ci) {
        return Some(String::new());
    }
    for sep in ['\\', '/'] {
        let prefix = format!("{root_full}{sep}");
        if starts_with(&full, &prefix, ci) {
            return Some(full[prefix.len()..].replace('\\', "/"));
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
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

fn starts_with(hay: &str, prefix: &str, ci: bool) -> bool {
    if hay.len() < prefix.len() {
        return false;
    }
    eq(&hay[..prefix.len()], prefix, ci)
}
