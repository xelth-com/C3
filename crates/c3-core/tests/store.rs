//! Store acceptance tests: the lock guards (A), the findings delta (B) and pending-record
//! keying/retention (C), exercised against a real temp directory.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use c3_core::findings::{Finding, FindingStatus};
use c3_core::ledger::LedgerEntry;
use c3_core::store::{
    CommitRequest, EvidenceStore, FilesStore, FindingsDelta, LockRecord, PendingRecord, PendingRef,
    PendingState, RecoveryDisposition, StatusChange,
};
use c3_core::task_slug::TaskSlug;

static SEQ: AtomicU64 = AtomicU64::new(0);

/// A fresh, unique temp collab root for one test.
fn temp_collab() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("c3-store-test-{pid}-{n}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn slug() -> TaskSlug {
    TaskSlug::new("demo-task").unwrap()
}

fn entry(n: i64, id: &str) -> LedgerEntry {
    LedgerEntry {
        n,
        consult_id: id.to_string(),
        ..Default::default()
    }
}

fn finding(id: &str) -> Finding {
    let mut f = Finding::default();
    f.id = id.to_string();
    f.severity = "blocker".into();
    f
}

// ------------------------------------------------------------------ A. lock guards

#[test]
fn second_task_lock_in_one_process_fails_fast() {
    let store = FilesStore::new(temp_collab());
    let task = slug();
    let rec = LockRecord::now(&task, None);
    let _held = store.take_task_lock(&task, &rec).expect("first task lock");
    // A second acquisition of the same ownership lock fails immediately (fail-fast).
    let err = store
        .take_task_lock(&task, &rec)
        .expect_err("second task lock must fail while the first is held");
    assert_eq!(err.kind(), std::io::ErrorKind::WouldBlock);
}

#[test]
fn write_lock_waits_and_reports_wait_ms() {
    let store = FilesStore::new(temp_collab());
    let task = slug();
    // The first acquisition is uncontended: wait_ms == 0.
    let lock1 = store.take_write_lock(&task).expect("first write lock");
    assert_eq!(lock1.wait_ms(), 0, "an uncontended write lock waits 0 ms");

    // Hold it in another thread for 200 ms, then release.
    let releaser = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        drop(lock1);
    });

    // The second acquisition must back off and report a measured wait.
    let lock2 = store
        .take_write_lock(&task)
        .expect("second write lock after wait");
    assert!(
        lock2.wait_ms() > 0,
        "a contended write lock reports wait_ms > 0, got {}",
        lock2.wait_ms()
    );
    releaser.join().unwrap();
}

// ------------------------------------------------------------------ B. findings delta

#[test]
fn two_deltas_from_one_snapshot_both_survive() {
    let store = FilesStore::new(temp_collab());
    let task = slug();

    // Both deltas are prepared from the same (empty) starting snapshot, before either
    // commit runs. Because commit re-reads findings.json under the lock and *appends* the
    // delta, the first finding is not erased by the second commit.
    let delta1 = FindingsDelta {
        new: vec![finding("F01-1")],
        ..Default::default()
    };
    let delta2 = FindingsDelta {
        new: vec![finding("F02-1")],
        ..Default::default()
    };

    let pending = PendingRef::single(task.clone());

    let lock = store.take_write_lock(&task).unwrap();
    store
        .commit(
            &lock,
            CommitRequest {
                entry: entry(1, "c1"),
                findings: delta1,
                pending: &pending,
                disposition: RecoveryDisposition::Remove,
                files: &[],
                bootstrap_cwd: "/repo".into(),
                bootstrap_tool: "codex-cli test".into(),
                commit_pause_ms: 0,
            },
        )
        .unwrap();
    drop(lock);

    let lock = store.take_write_lock(&task).unwrap();
    let receipt = store
        .commit(
            &lock,
            CommitRequest {
                entry: entry(2, "c2"),
                findings: delta2,
                pending: &pending,
                disposition: RecoveryDisposition::Remove,
                files: &[],
                bootstrap_cwd: "/repo".into(),
                bootstrap_tool: "codex-cli test".into(),
                commit_pause_ms: 0,
            },
        )
        .unwrap();
    drop(lock);
    assert!(receipt.committed);

    let findings = store.read_findings(&task).unwrap().unwrap();
    let ids: Vec<&str> = findings.findings.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, vec!["F01-1", "F02-1"], "both deltas survived");

    let sessions = store.read_sessions(&task).unwrap().unwrap();
    assert_eq!(sessions.codex.consults.len(), 2);
    assert_eq!(
        sessions.cwd, "/repo",
        "bootstrap cwd was recorded, not empty"
    );
    assert_eq!(sessions.codex.tool, "codex-cli test");
}

#[test]
fn update_findings_applies_a_status_change_only_transaction() {
    let store = FilesStore::new(temp_collab());
    let task = slug();
    let pending = PendingRef::single(task.clone());

    // Commit one finding first.
    let lock = store.take_write_lock(&task).unwrap();
    store
        .commit(
            &lock,
            CommitRequest {
                entry: entry(1, "c1"),
                findings: FindingsDelta {
                    new: vec![finding("F01-1")],
                    ..Default::default()
                },
                pending: &pending,
                disposition: RecoveryDisposition::Remove,
                files: &[],
                bootstrap_cwd: "/repo".into(),
                bootstrap_tool: "t".into(),
                commit_pause_ms: 0,
            },
        )
        .unwrap();
    drop(lock);

    // A findings-only transaction: verify it (with evidence) - no ledger entry.
    let lock = store.take_write_lock(&task).unwrap();
    store
        .update_findings(
            &lock,
            &task,
            &FindingsDelta {
                status_changes: vec![StatusChange {
                    id: "F01-1".into(),
                    to: FindingStatus::Verified,
                    note: String::new(),
                    evidence: "ran cargo test".into(),
                    when: "t1".into(),
                    by: "me".into(),
                }],
                ..Default::default()
            },
        )
        .unwrap();
    drop(lock);

    let findings = store.read_findings(&task).unwrap().unwrap();
    assert_eq!(findings.findings[0].status(), &FindingStatus::Verified);
    assert_eq!(findings.findings[0].history.len(), 1);
    // no extra ledger entry was written by a findings-only transaction
    assert_eq!(
        store
            .read_sessions(&task)
            .unwrap()
            .unwrap()
            .codex
            .consults
            .len(),
        1
    );
}

// ------------------------------------------------------------------ C. pending keying/retention

fn member_record(nn: u32, state: PendingState) -> PendingRecord {
    PendingRecord {
        state,
        nn: format!("{nn:02}"),
        n: nn as i64,
        ..Default::default()
    }
}

#[test]
fn committing_one_member_leaves_the_other() {
    let store = FilesStore::new(temp_collab());
    let task = slug();

    let m1 = PendingRef::member(task.clone(), 1);
    let m2 = PendingRef::member(task.clone(), 2);
    store
        .write_pending(&m1, &member_record(1, PendingState::Running))
        .unwrap();
    store
        .write_pending(&m2, &member_record(2, PendingState::Running))
        .unwrap();

    // Commit member 2 only; member 1's record must survive.
    let lock = store.take_write_lock(&task).unwrap();
    store
        .commit(
            &lock,
            CommitRequest {
                entry: entry(2, "c2"),
                findings: FindingsDelta::default(),
                pending: &m2,
                disposition: RecoveryDisposition::Remove,
                files: &[],
                bootstrap_cwd: "/repo".into(),
                bootstrap_tool: "t".into(),
                commit_pause_ms: 0,
            },
        )
        .unwrap();
    drop(lock);

    let recovered = store.recover_pending(&task).unwrap();
    let nns: Vec<Option<u32>> = recovered.iter().map(|r| r.nn).collect();
    assert_eq!(
        nns,
        vec![Some(1)],
        "member 1 survives, member 2 was removed"
    );
}

#[test]
fn a_survivors_record_survives_a_retain_commit() {
    let store = FilesStore::new(temp_collab());
    let task = slug();

    let m = PendingRef::member(task.clone(), 3);
    store
        .write_pending(&m, &member_record(3, PendingState::Survivors))
        .unwrap();

    // Commit with Retain: the survivors record (a possibly-live child) is not deleted.
    let lock = store.take_write_lock(&task).unwrap();
    store
        .commit(
            &lock,
            CommitRequest {
                entry: entry(3, "c3"),
                findings: FindingsDelta::default(),
                pending: &m,
                disposition: RecoveryDisposition::Retain,
                files: &[],
                bootstrap_cwd: "/repo".into(),
                bootstrap_tool: "t".into(),
                commit_pause_ms: 0,
            },
        )
        .unwrap();
    drop(lock);

    let recovered = store.recover_pending(&task).unwrap();
    assert_eq!(
        recovered.len(),
        1,
        "the survivors record survives a Retain commit"
    );
    assert_eq!(recovered[0].record.state, PendingState::Survivors);
    assert!(recovered[0].record.state.protects_child());
}

#[test]
fn recover_pending_is_path_bearing() {
    let store = FilesStore::new(temp_collab());
    let task = slug();
    let single = PendingRef::single(task.clone());
    store
        .write_pending(&single, &member_record(0, PendingState::Launching))
        .unwrap();
    let recovered = store.recover_pending(&task).unwrap();
    assert_eq!(recovered.len(), 1);
    assert!(recovered[0].path.ends_with(".consult.pending.json"));
    assert_eq!(recovered[0].nn, None);
}
