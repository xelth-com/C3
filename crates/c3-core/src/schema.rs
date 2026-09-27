//! The bundled consult-reply v1 JSON Schema.
//!
//! The PowerShell plugin ships `schemas/consult-reply.schema.json` next to its scripts and
//! passes that path to `codex exec --output-schema` (or inlines its text for the prompt-only
//! transport). C3 is a single binary with no plugin directory, so it embeds the same schema
//! file with [`include_str!`] ([`REPLY_SCHEMA_V1`], byte-for-byte the plugin's file) and
//! [`materialize`] writes it to a stable path under `CODEX_HOME` — `c3/schemas/
//! consult-reply.v1.json` — when the file is missing or its bytes differ, so `--output-schema`
//! can name a real file whose content is identical to the plugin's schema.

use std::io;
use std::path::{Path, PathBuf};

/// The consult-reply v1 schema, embedded verbatim from `schemas/consult-reply.schema.json`.
pub const REPLY_SCHEMA_V1: &str = include_str!("../schemas/consult-reply.schema.json");

/// The materialised schema's path under a codex home: `<codex_home>/c3/schemas/
/// consult-reply.v1.json`.
pub fn materialized_path(codex_home: &Path) -> PathBuf {
    codex_home
        .join("c3")
        .join("schemas")
        .join("consult-reply.v1.json")
}

/// Materialise [`REPLY_SCHEMA_V1`] under `codex_home` and return the file's path. The file is
/// (re)written only when it is missing or its current bytes differ from the embedded schema,
/// so an unchanged file keeps its mtime and no needless write happens. The returned path is
/// what a run passes to `--output-schema`.
pub fn materialize(codex_home: &Path) -> io::Result<PathBuf> {
    let path = materialized_path(codex_home);
    let want = REPLY_SCHEMA_V1.as_bytes();
    let up_to_date = std::fs::read(&path).map(|cur| cur == want).unwrap_or(false);
    if !up_to_date {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Atomic replace (temp + rename), matching the store's write discipline.
        let tmp = path.with_extension("v1.json.tmp");
        std::fs::write(&tmp, want)?;
        std::fs::rename(&tmp, &path)?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_the_plugin_file() {
        // The embedded schema is a well-formed JSON object describing the reply.
        let v: serde_json::Value = serde_json::from_str(REPLY_SCHEMA_V1).unwrap();
        assert_eq!(v["title"], "codex-consult reply v1");
        // LF line endings only — parity with the plugin's on-disk schema file.
        assert!(!REPLY_SCHEMA_V1.contains('\r'));
    }

    #[test]
    fn materialize_writes_then_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("c3-schema-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = materialize(&dir).unwrap();
        assert_eq!(p, materialized_path(&dir));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), REPLY_SCHEMA_V1);
        // A second call must not error and must leave identical bytes.
        let p2 = materialize(&dir).unwrap();
        assert_eq!(p, p2);
        assert_eq!(std::fs::read_to_string(&p2).unwrap(), REPLY_SCHEMA_V1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
