//! `--profile` resolution for `c3 snapshot` (Part B).
//!
//! `--profile a,b,-c,<glob>` reads `.eck/profiles.json`
//! (`{"name": {"include": [globs], "exclude": [globs]}}`) and resolves the comma-separated
//! tokens into one include set and one exclude set:
//! * a named profile unions its `include` globs into the includes and its `exclude` globs
//!   into the excludes;
//! * a leading `-` subtracts a profile — its `include` globs join the excludes;
//! * a token containing a glob metacharacter (`* ? [ ] { }`) is an ad-hoc include glob;
//! * an unknown profile name is refused, listing the known names.
//!
//! The resolution ([`resolve`]) and the match test ([`ProfileFilter::keep`]) are pure; the
//! only I/O is [`load_profiles`], which reads `.eck/profiles.json`. Applied after discovery,
//! before the budget/depth pass: a path is kept when it matches an include (or no include
//! glob was given at all) and does not match an exclude.

use std::collections::BTreeMap;
use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;

/// One named profile in `.eck/profiles.json`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// The whole `.eck/profiles.json` map (profile name → profile).
pub type Profiles = BTreeMap<String, Profile>;

/// A resolved profile selection: the union of include globs and exclude globs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolved {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

/// Split a `--profile` value into trimmed, non-empty tokens.
pub fn parse_tokens(csv: &str) -> Vec<String> {
    csv.split(',')
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

/// Whether a token is an ad-hoc include glob (it contains a glob metacharacter) rather than
/// a profile name reference.
fn is_glob_token(tok: &str) -> bool {
    tok.chars()
        .any(|c| matches!(c, '*' | '?' | '[' | ']' | '{' | '}'))
}

fn unknown(name: &str, profiles: &Profiles) -> String {
    let known: Vec<&str> = profiles.keys().map(|s| s.as_str()).collect();
    if known.is_empty() {
        format!("unknown profile '{name}' (.eck/profiles.json defines no profiles)")
    } else {
        format!("unknown profile '{name}' (known: {})", known.join(", "))
    }
}

/// Resolve the comma-separated `--profile` tokens against the profile map. Pure. Refuses an
/// unknown profile name with the known names listed.
pub fn resolve(tokens: &[String], profiles: &Profiles) -> Result<Resolved, String> {
    let mut include: Vec<String> = Vec::new();
    let mut exclude: Vec<String> = Vec::new();
    for tok in tokens {
        // An ad-hoc glob is a positive include regardless of a leading `-`.
        if is_glob_token(tok) {
            include.push(tok.clone());
            continue;
        }
        if let Some(name) = tok.strip_prefix('-') {
            let p = profiles.get(name).ok_or_else(|| unknown(name, profiles))?;
            exclude.extend(p.include.iter().cloned());
            continue;
        }
        let p = profiles.get(tok).ok_or_else(|| unknown(tok, profiles))?;
        include.extend(p.include.iter().cloned());
        exclude.extend(p.exclude.iter().cloned());
    }
    Ok(Resolved { include, exclude })
}

/// A compiled include/exclude matcher over repo-relative POSIX paths.
pub struct ProfileFilter {
    include: GlobSet,
    exclude: GlobSet,
    has_include: bool,
}

fn build_set(globs: &[String]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for g in globs {
        if let Ok(glob) = Glob::new(g) {
            b.add(glob);
        }
    }
    b.build().unwrap_or_else(|_| GlobSet::empty())
}

impl ProfileFilter {
    /// Compile a [`Resolved`] selection into a matcher.
    pub fn new(resolved: &Resolved) -> Self {
        ProfileFilter {
            include: build_set(&resolved.include),
            exclude: build_set(&resolved.exclude),
            has_include: !resolved.include.is_empty(),
        }
    }

    /// Whether a repo-relative POSIX path survives the profile: it matches an include (or no
    /// include glob was given) and matches no exclude.
    pub fn keep(&self, rel: &str) -> bool {
        if self.has_include && !self.include.is_match(rel) {
            return false;
        }
        !self.exclude.is_match(rel)
    }
}

/// Load `.eck/profiles.json`. Returns an error when `--profile` was requested but the file is
/// missing or malformed (the caller only calls this when tokens were given).
pub fn load_profiles(repo_root: &Path) -> Result<Profiles, String> {
    let path = repo_root.join(".eck").join("profiles.json");
    let text = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "--profile needs {}, which could not be read: {e}",
            path.display()
        )
    })?;
    serde_json::from_str::<Profiles>(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Profiles {
        let mut m = Profiles::new();
        m.insert(
            "core".into(),
            Profile {
                include: vec!["crates/c3/**".into()],
                exclude: vec!["crates/c3/**/tests/**".into()],
            },
        );
        m.insert(
            "docs".into(),
            Profile {
                include: vec!["docs/**".into(), "*.md".into()],
                exclude: vec![],
            },
        );
        m.insert(
            "noise".into(),
            Profile {
                include: vec!["**/generated/**".into()],
                exclude: vec![],
            },
        );
        m
    }

    #[test]
    fn named_profiles_union_includes_and_carry_excludes() {
        let r = resolve(&parse_tokens("core,docs"), &sample()).unwrap();
        assert_eq!(
            r.include,
            vec!["crates/c3/**", "docs/**", "*.md"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
        assert_eq!(r.exclude, vec!["crates/c3/**/tests/**".to_string()]);
    }

    #[test]
    fn leading_dash_subtracts_a_profiles_includes() {
        let r = resolve(&parse_tokens("core,-noise"), &sample()).unwrap();
        assert_eq!(r.include, vec!["crates/c3/**".to_string()]);
        assert!(r.exclude.contains(&"**/generated/**".to_string()));
        assert!(r.exclude.contains(&"crates/c3/**/tests/**".to_string()));
    }

    #[test]
    fn a_glob_token_is_an_adhoc_include() {
        let r = resolve(&parse_tokens("core,src/*.rs"), &sample()).unwrap();
        assert!(r.include.contains(&"src/*.rs".to_string()));
    }

    #[test]
    fn unknown_profile_is_refused_with_known_names() {
        let err = resolve(&parse_tokens("core,bogus"), &sample()).unwrap_err();
        assert!(err.contains("bogus"), "{err}");
        assert!(err.contains("core"), "{err}");
        assert!(err.contains("docs"), "{err}");
    }

    #[test]
    fn filter_keeps_include_minus_exclude() {
        let r = resolve(&parse_tokens("core"), &sample()).unwrap();
        let f = ProfileFilter::new(&r);
        assert!(f.keep("crates/c3/src/lib.rs"));
        assert!(!f.keep("crates/c3/src/tests/x.rs"));
        assert!(!f.keep("crates/other/src/lib.rs"));
    }

    #[test]
    fn filter_with_no_includes_keeps_everything_but_excludes() {
        let r = resolve(&parse_tokens("-noise"), &sample()).unwrap();
        let f = ProfileFilter::new(&r);
        assert!(f.keep("src/main.rs"));
        assert!(!f.keep("src/generated/pb.rs"));
    }
}
