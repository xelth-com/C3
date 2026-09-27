//! File discovery for the pack pipeline (ported from eckSnapshot's `fileUtils.js`
//! discovery + `snapshotBuilder.js` `discoverFiles`).
//!
//! The walk respects every `.gitignore` (nested, `.git/info/exclude`, global excludes)
//! through the `ignore` crate, then applies C3's own hard-ignore list *regardless* of
//! `.gitignore` (DESIGN §3 invariant 5): secrets and key material (`.env*`, `*.pem`,
//! `*.key`, `id_rsa*`, `*.p12`, `*.pfx`), consult lock/pending files
//! (`.collab/**/.consult.*`), index and build directories, and the logs/dumps/swap globs
//! eckSnapshot hard-ignores (`GLOBAL_HARD_IGNORE_*`). A content-aware binary sniff (magic
//! bytes + a null-byte heuristic over the first 8 KiB) drops binaries the extension check
//! misses, and a size cap drops oversized files. Every returned path is repo-relative and
//! POSIX (`/`-separated), on every OS (invariant 9).

use std::path::{Path, PathBuf};

use globset::{GlobSet, GlobSetBuilder};

use c3_core::paths::repo_relative;

/// One file that passed every filter.
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Repo-relative POSIX path (`crates/c3/src/lib.rs`).
    pub rel: String,
    /// Absolute path on disk.
    pub abs: PathBuf,
    /// Size in bytes.
    pub size: u64,
}

/// Discovery options.
#[derive(Debug, Clone)]
pub struct DiscoverOpts {
    /// Skip any file larger than this many bytes (default 2 MiB).
    pub max_file_size: u64,
    /// Keep binary files (they are dropped by default).
    pub include_binary: bool,
}

impl Default for DiscoverOpts {
    fn default() -> Self {
        DiscoverOpts {
            max_file_size: 2 * 1024 * 1024,
            include_binary: false,
        }
    }
}

/// Directory names hard-ignored regardless of `.gitignore` (eckSnapshot
/// `GLOBAL_HARD_IGNORE_DIRS` plus C3's build/index dirs).
const HARD_IGNORE_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".idea",
    ".vscode",
    ".gradle",
    "build",
    "__pycache__",
    ".c3", // C3's own index / anchor / snapshot store under .collab or elsewhere
];

/// Exact file names hard-ignored (lockfiles; eckSnapshot `GLOBAL_HARD_IGNORE_FILES`).
const HARD_IGNORE_FILES: &[&str] = &["package-lock.json", "yarn.lock", "pnpm-lock.yaml", "go.sum"];

/// Glob patterns matched against the file *basename*, hard-ignored regardless of
/// `.gitignore`: key material, and eckSnapshot's logs/dumps/swap `GLOBAL_HARD_IGNORE_GLOBS`.
const HARD_IGNORE_BASENAME_GLOBS: &[&str] = &[
    // Secrets / key material (DESIGN §3.5, D8).
    ".env",
    ".env.*",
    "*.pem",
    "*.key",
    "id_rsa*",
    "id_dsa*",
    "id_ecdsa*",
    "id_ed25519*",
    "*.p12",
    "*.pfx",
    // eckSnapshot GLOBAL_HARD_IGNORE_GLOBS: rotated logs, core dumps, editor swap files.
    "*.log",
    "*.log.[0-9]*",
    "*.log.gz",
    "*.log.*.gz",
    "*.log.bz2",
    "*.log.xz",
    "core.[0-9]*",
    "*.swp",
    "*.swo",
];

/// Glob patterns matched against the full repo-relative path (consult lock/pending files
/// live under the collab dir).
const HARD_IGNORE_PATH_GLOBS: &[&str] = &["**/.consult.*", ".consult.*"];

fn build_set(patterns: &[&str]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        // Case-insensitive to match eckSnapshot's `{ nocase: true }`.
        b.add(
            globset::GlobBuilder::new(p)
                .case_insensitive(true)
                .literal_separator(false)
                .build()
                .unwrap(),
        );
    }
    b.build().unwrap()
}

fn build_path_set(patterns: &[&str]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(
            globset::GlobBuilder::new(p)
                .case_insensitive(true)
                .literal_separator(true)
                .build()
                .unwrap(),
        );
    }
    b.build().unwrap()
}

/// Whether a directory name is hard-ignored.
fn is_hard_ignored_dir(name: &str) -> bool {
    HARD_IGNORE_DIRS
        .iter()
        .any(|d| d.eq_ignore_ascii_case(name))
}

/// Walk `repo_root` and return every file that passes gitignore, the hard-ignore list, the
/// binary sniff and the size cap, as repo-relative POSIX paths sorted lexicographically.
pub fn discover(repo_root: &Path, opts: &DiscoverOpts) -> Vec<FileEntry> {
    let basename_set = build_set(HARD_IGNORE_BASENAME_GLOBS);
    let path_set = build_path_set(HARD_IGNORE_PATH_GLOBS);

    let mut walker = ignore::WalkBuilder::new(repo_root);
    walker
        .standard_filters(true) // .gitignore, .ignore, hidden-git, parents
        .hidden(false) // do NOT drop dotfiles wholesale; the hard-ignore list decides
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .require_git(false)
        .parents(true);

    // Prune hard-ignored directories by name so we never descend into target/ etc.
    walker.filter_entry(|entry| {
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if let Some(name) = entry.file_name().to_str() {
                return !is_hard_ignored_dir(name);
            }
        }
        true
    });

    let mut out: Vec<FileEntry> = Vec::new();
    for result in walker.build() {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let abs = entry.path();
        let rel = match repo_relative(repo_root, abs) {
            Some(r) if !r.is_empty() => r,
            _ => continue,
        };
        let name = abs.file_name().and_then(|n| n.to_str()).unwrap_or_default();

        if HARD_IGNORE_FILES
            .iter()
            .any(|f| f.eq_ignore_ascii_case(name))
        {
            continue;
        }
        if basename_set.is_match(name) {
            continue;
        }
        if path_set.is_match(&rel) {
            continue;
        }

        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if size > opts.max_file_size {
            continue;
        }
        if !opts.include_binary && is_binary_file(abs) {
            continue;
        }

        out.push(FileEntry {
            rel,
            abs: abs.to_path_buf(),
            size,
        });
    }

    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    out
}

/// Binary magic-byte signatures (ported from eckSnapshot's `BINARY_MAGIC_NUMBERS`).
const BINARY_MAGIC: &[&[u8]] = &[
    &[0x7F, 0x45, 0x4C, 0x46],                         // ELF
    &[0x4D, 0x5A],                                     // PE/EXE/DLL
    b"SQLite format 3\0",                              // SQLite 3
    &[0xCA, 0xFE, 0xBA, 0xBE],                         // Java class / Mach-O fat
    &[0xFE, 0xED, 0xFA, 0xCE],                         // Mach-O 32
    &[0xFE, 0xED, 0xFA, 0xCF],                         // Mach-O 64
    &[0xCF, 0xFA, 0xED, 0xFE],                         // Mach-O 64 LE
    &[0xCE, 0xFA, 0xED, 0xFE],                         // Mach-O 32 LE
    &[0x00, 0x61, 0x73, 0x6D],                         // WebAssembly
    &[0x50, 0x4B, 0x03, 0x04],                         // ZIP / JAR / docx
    &[0x50, 0x4B, 0x05, 0x06],                         // ZIP empty
    &[0x50, 0x4B, 0x07, 0x08],                         // ZIP spanned
    &[0x1F, 0x8B],                                     // GZIP
    &[0x42, 0x5A, 0x68],                               // BZIP2
    &[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00],             // XZ
    &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C],             // 7-zip
    &[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07],             // RAR
    &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1], // MS Compound
    &[0x89, 0x50, 0x4E, 0x47],                         // PNG
    &[0xFF, 0xD8, 0xFF],                               // JPEG
];

/// Content-aware binary sniff over the first 8 KiB: magic bytes then a null-byte
/// heuristic. An empty or unreadable file is treated as text.
pub fn is_binary_file(path: &Path) -> bool {
    use std::io::Read;
    let mut buf = [0u8; 8192];
    let n = match std::fs::File::open(path).and_then(|mut f| f.read(&mut buf)) {
        Ok(n) => n,
        Err(_) => return false,
    };
    if n == 0 {
        return false;
    }
    let sample = &buf[..n];
    for magic in BINARY_MAGIC {
        if sample.len() >= magic.len() && &sample[..magic.len()] == *magic {
            return true;
        }
    }
    sample.contains(&0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("c3-pack-discover-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn hard_ignores_env_and_keys_even_when_tracked() {
        let d = scratch("hardignore");
        fs::write(d.join("keep.rs"), "fn a() {}").unwrap();
        fs::write(d.join(".env"), "SECRET=1").unwrap();
        fs::write(d.join(".env.local"), "SECRET=2").unwrap();
        fs::write(d.join("server.pem"), "-----BEGIN CERT-----").unwrap();
        fs::write(d.join("deploy.key"), "x").unwrap();
        fs::write(d.join("id_rsa"), "x").unwrap();
        // No .gitignore at all → gitignore does not save us; the hard-ignore list must.
        let files = discover(&d, &DiscoverOpts::default());
        let rels: Vec<&str> = files.iter().map(|f| f.rel.as_str()).collect();
        assert!(rels.contains(&"keep.rs"), "got {rels:?}");
        assert!(!rels.iter().any(|r| r.starts_with(".env")));
        assert!(!rels.contains(&"server.pem"));
        assert!(!rels.contains(&"deploy.key"));
        assert!(!rels.contains(&"id_rsa"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn respects_gitignore_and_prunes_target() {
        let d = scratch("gitignore");
        fs::write(d.join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(d.join("ignored.txt"), "x").unwrap();
        fs::write(d.join("kept.txt"), "x").unwrap();
        fs::create_dir_all(d.join("target/debug")).unwrap();
        fs::write(d.join("target/debug/art.rs"), "fn a() {}").unwrap();
        let files = discover(&d, &DiscoverOpts::default());
        let rels: Vec<&str> = files.iter().map(|f| f.rel.as_str()).collect();
        assert!(rels.contains(&"kept.txt"), "got {rels:?}");
        assert!(!rels.contains(&"ignored.txt"));
        assert!(!rels.iter().any(|r| r.starts_with("target/")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn binary_sniff_drops_null_bytes_and_magic() {
        let d = scratch("binary");
        fs::write(d.join("text.rs"), "fn a() {}\n").unwrap();
        fs::write(d.join("nulls.bin"), [0x01, 0x00, 0x02, 0x03]).unwrap();
        fs::write(d.join("elf"), [0x7F, 0x45, 0x4C, 0x46, 0x10]).unwrap();
        let files = discover(&d, &DiscoverOpts::default());
        let rels: Vec<&str> = files.iter().map(|f| f.rel.as_str()).collect();
        assert!(rels.contains(&"text.rs"), "got {rels:?}");
        assert!(!rels.contains(&"nulls.bin"));
        assert!(!rels.contains(&"elf"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn is_binary_file_detects_null_byte() {
        let d = scratch("isbin");
        let p = d.join("f");
        fs::write(&p, b"hello\x00world").unwrap();
        assert!(is_binary_file(&p));
        fs::write(&p, b"hello world").unwrap();
        assert!(!is_binary_file(&p));
        let _ = fs::remove_dir_all(&d);
    }
}
