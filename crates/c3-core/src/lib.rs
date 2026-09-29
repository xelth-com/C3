//! C3 core: contracts and formats, dependency-light.
//!
//! This crate holds the domain types and the file/format readers the C3 runtime
//! builds on: the constrained Codex config scanner ([`config`]), the reviewer
//! roster ([`roster`]), effort vocabularies ([`effort`]), reviewer lineage and
//! identity ([`lineage`]), the recorded endpoint health ([`health`]), the preflight
//! verdict ([`verdict`]), purposes ([`purpose`]), path normalisation ([`paths`]) and
//! the task-slug newtype ([`task_slug`]).
//!
//! Most of this crate is pure over its inputs: it reads no environment, spawns no
//! process and touches no file system - the config scanner, roster, effort, lineage,
//! health, verdict, purpose, path and the ledger/findings/handoff/engine *format*
//! types. The one deliberate exception is [`store`]: [`store::EvidenceStore`] is the
//! file-and-lock boundary (atomic writes, the ownership/write locks, the commit write
//! order), so it necessarily touches the file system and takes real OS locks. The rest
//! of the runtime side (launcher discovery, `codex login status`, `agy models`, muse
//! `auth.json`, subprocess launch) lives in the `c3` crate. This mirrors the PowerShell
//! bridge's `codex-consult-common.ps1`, the reference these formats are ported from
//! byte-for-byte.

pub mod availability;
pub mod config;
pub mod credential;
pub mod effort;
pub mod engine;
pub mod findings;
pub mod handoff;
pub mod health;
pub mod host;
pub mod ledger;
pub mod lineage;
pub mod paths;
pub mod peak;
pub mod ps_json;
pub mod purpose;
pub mod roster;
pub mod roster_ext;
pub mod schema;
pub mod store;
pub mod task_slug;
pub mod test_hooks;
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
