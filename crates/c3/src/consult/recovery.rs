//! The recovery-record pass a consultation runs before it launches (`codex-consult.ps1`
//! "recovery records", ~2766-2909). Every `.consult.pending*.json` of the task is read and
//! judged (`crate::liveness::pending`): an unusable record refuses the run (corruption); an
//! ACTIVE record — the bridge or codex process of an interrupted run is still alive — refuses
//! a real run (its message names the pid); a dead record is *consumed*: numbering skips past
//! its `n`/`nn` (already handled by `next_numbers`, which reads the same records), a
//! recovered/cleared line is printed, and a consumed panel-member record is removed.
//!
//! "Recovered" vs "cleared" mirrors `Get-NextNumbers`'s per-leftover `Recovered` flag: a
//! record is *recovered* when its `n` never reached the ledger (a run that stopped before its
//! commit point), else *cleared* (its ledger entry exists — a failed registration, or timeout
//! survivors that have since exited). Orphan findings (a finding whose `source.consult` has no
//! ledger entry) are not touched here: numbering never reuses such a number, and
//! `c3 findings -List` is what flags them (M3).

use std::collections::HashSet;
use std::path::PathBuf;

use c3_core::store::{EvidenceStore, FilesStore};
use c3_core::task_slug::TaskSlug;

use crate::liveness::pending;

/// One recovery record read and judged.
pub struct RecoveryItem {
    pub path: PathBuf,
    pub name: String,
    pub state: String,
    pub n: i64,
    pub nn: String,
    /// Its `n` never reached the ledger (a run stopped before its commit point).
    pub recovered: bool,
    /// A live process of the interrupted run still exists.
    pub active: bool,
    /// The `Check` text (the reason it is judged inactive/active), for the console line.
    pub check: String,
    /// The refusal message, when active.
    pub message: String,
}

/// The whole recovery assessment of a task.
pub struct Recovery {
    /// The first unusable record's refusal (corruption); a real run refuses on it.
    pub error: Option<String>,
    pub items: Vec<RecoveryItem>,
}

impl Recovery {
    /// The first active record's refusal message; a real run refuses on it (its pid is named).
    pub fn active_message(&self) -> Option<String> {
        self.items
            .iter()
            .find(|i| i.active)
            .map(|i| i.message.clone())
    }
}

/// Read and judge every recovery record of the task (`Read-TaskPendingRecords` + the
/// `Recovered` flag from `Get-NextNumbers`).
pub fn assess(store: &FilesStore, task: &TaskSlug) -> Recovery {
    let task_dir = store.task_dir(task);
    // Ledger `n`s (for the recovered/cleared distinction).
    let ledger_ns: HashSet<i64> = store
        .read_sessions(task)
        .ok()
        .flatten()
        .map(|s| s.codex.consults.iter().map(|e| e.n).collect())
        .unwrap_or_default();

    let mut items = Vec::new();
    for path in pending::pending_paths(&task_dir) {
        let rd = pending::read_pending_file(&path);
        if let Some(err) = rd.error {
            return Recovery {
                error: Some(err),
                items,
            };
        }
        let record = match rd.record {
            Some(r) => r,
            None => continue,
        };
        let check = pending::test_pending_active(&record, &path);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let state = record
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let n = record.get("n").and_then(|v| v.as_i64()).unwrap_or(0);
        let nn = record
            .get("nn")
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default();
        // Recovered: n>0 with no ledger entry, or (no usable n) an nn that is present.
        let recovered = if n > 0 {
            !ledger_ns.contains(&n)
        } else {
            !nn.is_empty()
        };
        items.push(RecoveryItem {
            path,
            name,
            state,
            n,
            nn,
            recovered,
            active: check.active,
            check: check.check,
            message: check.message,
        });
    }
    Recovery { error: None, items }
}

/// `"<name>: "` for a panel member's record, `""` for the classic single-run record.
fn which(item: &RecoveryItem) -> String {
    if item.name.eq_ignore_ascii_case(".consult.pending.json") {
        String::new()
    } else {
        format!("{}: ", item.name)
    }
}

fn num(n: i64) -> String {
    if n > 0 {
        n.to_string()
    } else {
        String::new()
    }
}

/// The console/handoff line for a consumed record on a real run (`$recoveredLines`, the
/// non-dry branch): "recovered reservation ..." or "cleared the recovery record ...".
pub fn run_line(item: &RecoveryItem) -> String {
    if item.recovered {
        format!(
            "recovered reservation n={}, nn={} ({}state '{}' of an interrupted run; {}); numbering continues past it.",
            num(item.n),
            item.nn,
            which(item),
            item.state,
            item.check
        )
    } else {
        format!(
            "cleared the recovery record of consult n={}, nn={} ({}state '{}'; {}); its ledger entry exists.",
            num(item.n),
            item.nn,
            which(item),
            item.state,
            item.check
        )
    }
}

/// The dry-run `pending :` line (the dry branch of `$recoveredLines`): an active record
/// reports the refusal, a dead one reports the recovery.
pub fn dry_line(item: &RecoveryItem) -> String {
    if item.active {
        format!(
            "{} (state '{}', n={}, nn={}): the next run would be REFUSED - {}",
            item.path.display(),
            item.state,
            num(item.n),
            item.nn,
            item.message
        )
    } else {
        format!(
            "{} (state '{}', n={}, nn={}; {}): the next run recovers it; numbering continues past it.",
            item.path.display(),
            item.state,
            num(item.n),
            item.nn,
            item.check
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch() -> (PathBuf, FilesStore, TaskSlug) {
        let dir = std::env::temp_dir().join(format!(
            "c3-recovery-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let task = TaskSlug::new("t").unwrap();
        std::fs::create_dir_all(dir.join("t")).unwrap();
        (dir.clone(), FilesStore::new(dir), task)
    }

    fn seed(store: &FilesStore, task: &TaskSlug, json: &str) -> PathBuf {
        let p = store.task_dir(task).join(".consult.pending.json");
        std::fs::write(&p, json).unwrap();
        p
    }

    #[test]
    fn reserved_dead_record_is_recovered_not_active() {
        let (root, store, task) = scratch();
        // reserved with an empty start_time: the writer-pid rule is skipped, the reserved
        // branch judges it inactive. No sessions.json, so n=9 is unrecovered → recovered.
        seed(
            &store,
            &task,
            r#"{"state":"reserved","n":9,"nn":"20","reply":"handoffs/20-codex-old.md","started":"","pid":1,"start_time":"","host":"OTHER","launcher":"","child_pid":null,"child_start_time":"","survivors":[],"note":""}"#,
        );
        let rec = assess(&store, &task);
        assert!(rec.error.is_none());
        assert!(
            rec.active_message().is_none(),
            "a reserved dead run is inactive"
        );
        assert_eq!(rec.items.len(), 1);
        let it = &rec.items[0];
        assert!(it.recovered, "n=9 has no ledger entry → recovered");
        assert!(!it.active);
        assert!(run_line(it).starts_with("recovered reservation n=9, nn=20 (state 'reserved'"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn running_record_with_live_child_is_active() {
        let (root, store, task) = scratch();
        // A running record naming THIS process as the child, with our real start time: the
        // recorded-pid rule finds it alive → active → a real run is refused.
        let me = std::process::id();
        let start = crate::liveness::proc::process_start_iso(me).unwrap_or_default();
        let host = c3_core::host::machine_name();
        let json = format!(
            r#"{{"state":"running","n":5,"nn":"07","reply":"handoffs/07-codex-old.md","started":"2026-01-01T00:00:00+00:00","pid":1,"start_time":"","host":"{host}","launcher":"","child_pid":{me},"child_start_time":"{start}","survivors":[],"note":""}}"#
        );
        seed(&store, &task, &json);
        let rec = assess(&store, &task);
        assert!(rec.error.is_none());
        let msg = rec
            .active_message()
            .expect("a live recorded child is active");
        assert!(
            msg.contains(&me.to_string()),
            "the refusal names the pid: {msg}"
        );
        assert!(rec.items[0].active);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_record_refuses() {
        let (root, store, task) = scratch();
        seed(&store, &task, "{ this is not json");
        let rec = assess(&store, &task);
        let err = rec.error.expect("a corrupt record refuses");
        assert!(err.contains("unusable"), "{err}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cleared_line_when_ledger_entry_exists() {
        // Formatting only: a non-recovered (cleared) record reads "cleared the recovery record".
        let it = RecoveryItem {
            path: PathBuf::from("/x/.consult.pending.json"),
            name: ".consult.pending.json".into(),
            state: "survivors".into(),
            n: 4,
            nn: "11".into(),
            recovered: false,
            active: false,
            check: "codex pid(s) 1234 no longer running".into(),
            message: String::new(),
        };
        assert_eq!(
            run_line(&it),
            "cleared the recovery record of consult n=4, nn=11 (state 'survivors'; codex pid(s) 1234 no longer running); its ledger entry exists."
        );
    }

    #[test]
    fn member_record_line_names_the_file() {
        let it = RecoveryItem {
            path: PathBuf::from("/x/.consult.pending-07.json"),
            name: ".consult.pending-07.json".into(),
            state: "reserved".into(),
            n: 9,
            nn: "07".into(),
            recovered: true,
            active: false,
            check: "reserved: codex was never started".into(),
            message: String::new(),
        };
        assert!(run_line(&it).contains("(.consult.pending-07.json: state 'reserved'"));
    }
}
