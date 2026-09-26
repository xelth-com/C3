//! C3 core: contracts and formats, dependency-light.
//!
//! This crate holds the domain types and the file/format readers the C3 runtime
//! builds on: the constrained Codex config scanner ([`config`]), the reviewer
//! roster ([`roster`]), effort vocabularies ([`effort`]), reviewer lineage and
//! identity ([`lineage`]), the recorded endpoint health ([`health`]), the preflight
//! verdict ([`verdict`]), purposes ([`purpose`]) and path normalisation ([`paths`]).
//!
//! Every function here is pure over its inputs: it reads no environment, spawns no
//! process and touches no file system. The runtime side (launcher discovery, `codex
//! login status`, `agy models`, muse `auth.json`, reading the ledgers) lives in the
//! `c3` crate. This mirrors the PowerShell bridge's `codex-consult-common.ps1`, the
//! reference these formats are ported from byte-for-byte.

pub mod availability;
pub mod config;
pub mod credential;
pub mod effort;
pub mod health;
pub mod lineage;
pub mod paths;
pub mod purpose;
pub mod roster;
pub mod verdict;

use sha2::{Digest, Sha256};

/// Lowercase hex SHA-256 of the bytes (`Get-Sha256Hex`).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    let mut s = String::with_capacity(out.len() * 2);
    for b in out {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Collapse every run of whitespace to a single space and trim (`ConvertTo-OneLine`).
pub fn one_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_ws = false;
    for c in text.chars() {
        if c.is_whitespace() {
            in_ws = true;
        } else {
            if in_ws && !out.is_empty() {
                out.push(' ');
            }
            in_ws = false;
            out.push(c);
        }
    }
    out
}
