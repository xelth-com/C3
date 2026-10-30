//! The git write/read commands `c3 finish` needs. `c3 finish` is the only path that writes git
//! refs/commits (the one exception to "C3 never commits"). It is NOT the only path that touches
//! git's object store: `c3 work` runs `git hash-object -w` to write loose objects for its
//! `patch.diff` baseline (see `work::record`), which creates dangling objects but no ref and no
//! commit. Every other code path reads git read-only.

use std::path::Path;
use std::process::Command;

/// Run `git -C <repo> <args>`, returning `(success, stdout_bytes, stderr_string)`.
fn git(repo_root: &Path, args: &[&str]) -> Result<(bool, Vec<u8>, String), String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.first().copied().unwrap_or("")))?;
    Ok((
        out.status.success(),
        out.stdout,
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    ))
}

/// `git status --porcelain=v1 -z` raw bytes.
pub fn status_porcelain_z(repo_root: &Path) -> Result<Vec<u8>, String> {
    let (ok, stdout, err) = git(repo_root, &["status", "--porcelain=v1", "-z"])?;
    if ok {
        Ok(stdout)
    } else {
        Err(format!("git status failed: {err}"))
    }
}

/// `git add -A` (honours `.gitignore`).
pub fn add_all(repo_root: &Path) -> Result<(), String> {
    let (ok, _, err) = git(repo_root, &["add", "-A"])?;
    if ok {
        Ok(())
    } else {
        Err(format!("git add -A failed: {err}"))
    }
}

/// Unstage `paths` from the index. Uses `git reset -q -- <paths>`; on an unborn branch
/// (no HEAD yet) it falls back to `git rm --cached`.
pub fn unstage(repo_root: &Path, paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut args: Vec<&str> = vec!["reset", "-q", "--"];
    for p in paths {
        args.push(p.as_str());
    }
    let (ok, _, _) = git(repo_root, &args)?;
    if ok {
        return Ok(());
    }
    // Unborn branch: reset against a missing HEAD fails; drop the entries instead.
    let mut args: Vec<&str> = vec!["rm", "--cached", "-q", "--force", "--"];
    for p in paths {
        args.push(p.as_str());
    }
    let (ok, _, err) = git(repo_root, &args)?;
    if ok {
        Ok(())
    } else {
        Err(format!("could not unstage: {err}"))
    }
}

/// The staged changes as `(status_char, path)` from `git diff --cached --name-status -z`.
/// For a rename/copy the NEW path is reported.
pub fn staged_name_status(repo_root: &Path) -> Result<Vec<(char, String)>, String> {
    let (ok, stdout, err) = git(repo_root, &["diff", "--cached", "--name-status", "-z"])?;
    if !ok {
        return Err(format!("git diff --cached failed: {err}"));
    }
    let s = String::from_utf8_lossy(&stdout);
    let mut it = s.split('\0').filter(|t| !t.is_empty());
    let mut out = Vec::new();
    while let Some(status) = it.next() {
        let c = status.chars().next().unwrap_or('?');
        if c == 'R' || c == 'C' {
            // old, then new — report the new path.
            let _old = it.next();
            if let Some(new) = it.next() {
                out.push((c, new.replace('\\', "/")));
            }
        } else if let Some(path) = it.next() {
            out.push((c, path.replace('\\', "/")));
        }
    }
    Ok(out)
}

/// `git commit -m <message>` (with `--allow-empty` when `allow_empty`).
pub fn commit(repo_root: &Path, message: &str, allow_empty: bool) -> Result<(), String> {
    let mut args: Vec<&str> = vec!["commit", "-m", message];
    if allow_empty {
        args.push("--allow-empty");
    }
    let (ok, _, err) = git(repo_root, &args)?;
    if ok {
        Ok(())
    } else {
        Err(format!("git commit failed: {err}"))
    }
}

/// `git rev-parse HEAD`, trimmed, or `None`.
pub fn head(repo_root: &Path) -> Option<String> {
    let (ok, stdout, _) = git(repo_root, &["rev-parse", "HEAD"]).ok()?;
    if !ok {
        return None;
    }
    let s = String::from_utf8_lossy(&stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn name_status_parses_renames_and_plain_entries() {
        // "M\0a.rs\0R100\0old.rs\0new.rs\0A\0b.rs\0"
        let mut buf = Vec::new();
        for tok in ["M", "a.rs", "R100", "old.rs", "new.rs", "A", "b.rs"] {
            buf.extend_from_slice(tok.as_bytes());
            buf.push(0);
        }
        let s = String::from_utf8_lossy(&buf);
        let mut it = s.split('\0').filter(|t| !t.is_empty());
        let mut out = Vec::new();
        while let Some(status) = it.next() {
            let c = status.chars().next().unwrap_or('?');
            if c == 'R' || c == 'C' {
                let _ = it.next();
                if let Some(new) = it.next() {
                    out.push((c, new.to_string()));
                }
            } else if let Some(p) = it.next() {
                out.push((c, p.to_string()));
            }
        }
        assert_eq!(
            out,
            vec![
                ('M', "a.rs".to_string()),
                ('R', "new.rs".to_string()),
                ('A', "b.rs".to_string()),
            ]
        );
    }
}
