use super::*;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "c3-statedir-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn state_components_compare_like_the_file_system() {
    assert!(is_state_component(".c3"));
    assert!(is_state_component(".eck"));
    assert!(!is_state_component(".collab"));
    assert!(!is_state_component("c3"));
    let insensitive = cfg!(any(windows, target_os = "macos"));
    assert_eq!(is_state_component(".C3"), insensitive);
    assert_eq!(is_state_component(".Eck"), insensitive);
}

#[test]
fn a_gitignore_line_that_ignores_the_folder_counts() {
    let r = scratch("gi");
    for body in [".c3/\n", ".c3\n", "/.c3/\n", ".*\n", "**/.c3\n"] {
        assert!(gitignore_ignores_state_dir(&r, body), "{body:?}");
    }
    for body in ["", ".eck/\n", "target/\n", ".c3/x\n", ".*\n!.c3/\n"] {
        assert!(!gitignore_ignores_state_dir(&r, body), "{body:?}");
    }
    let _ = std::fs::remove_dir_all(&r);
}

// `.c3ignore` wins over `.eckignore` in the same directory; a directory with only `.eckignore`
// is still honoured; the deeper file decides.
#[test]
fn c3ignore_wins_and_eckignore_is_the_fallback() {
    let r = scratch("ignore");
    std::fs::write(r.join(LEGACY_IGNORE_FILE), "legacy.md\n").unwrap();
    let mut ci = ContextIgnore::new(&r);
    assert!(ci.excluded("legacy.md", false));
    assert!(!ci.excluded("other.md", false));
    std::fs::write(r.join(IGNORE_FILE), "other.md\n").unwrap();
    let mut ci = ContextIgnore::new(&r);
    assert!(ci.excluded("other.md", false));
    assert!(
        !ci.excluded("legacy.md", false),
        ".eckignore is not read next to a .c3ignore"
    );
    std::fs::create_dir_all(r.join("sub")).unwrap();
    std::fs::write(r.join("sub").join(LEGACY_IGNORE_FILE), "!other.md\nx/\n").unwrap();
    let mut ci = ContextIgnore::new(&r);
    assert!(!ci.excluded("sub/other.md", false));
    assert!(ci.excluded("sub/x/deep.rs", false));
    assert!(ci.excluded("sub/x", true));
    let _ = std::fs::remove_dir_all(&r);
}

#[test]
fn the_read_path_falls_back_to_the_legacy_folder() {
    let r = scratch("read");
    assert_eq!(read_path(&r), None);
    assert_eq!(layout(&r), Layout::Absent);
    std::fs::create_dir_all(legacy_path(&r)).unwrap();
    assert_eq!(read_path(&r), Some(legacy_path(&r)));
    assert_eq!(layout(&r), Layout::LegacyOnly);
    std::fs::create_dir_all(path(&r)).unwrap();
    assert_eq!(read_path(&r), Some(path(&r)));
    assert_eq!(layout(&r), Layout::Both);
    std::fs::remove_dir_all(legacy_path(&r)).unwrap();
    assert_eq!(layout(&r), Layout::Current);
    assert_eq!(label(&path(&r), "ROADMAP.md"), ".c3/ROADMAP.md");
    let _ = std::fs::remove_dir_all(&r);
}

// The legacy collab cache moves entry by entry; an existing destination wins and is named.
#[test]
fn a_legacy_collab_cache_moves_entry_by_entry() {
    let r = scratch("cache");
    let collab = r.join(".collab");
    std::fs::create_dir_all(collab.join(STATE_DIR).join("index")).unwrap();
    std::fs::write(collab.join(STATE_DIR).join("anchor"), "abc\n").unwrap();
    std::fs::write(collab.join(STATE_DIR).join("snapshot_seq"), "3").unwrap();
    std::fs::create_dir_all(path(&r)).unwrap();
    std::fs::write(path(&r).join("snapshot_seq"), "9").unwrap();
    let notes = migrate_collab_cache(&r, &collab);
    assert_eq!(
        std::fs::read_to_string(path(&r).join("anchor")).unwrap(),
        "abc\n"
    );
    assert!(path(&r).join("index").is_dir());
    assert_eq!(
        std::fs::read_to_string(path(&r).join("snapshot_seq")).unwrap(),
        "9"
    );
    assert!(collab.join(STATE_DIR).join("snapshot_seq").is_file());
    assert!(
        notes
            .iter()
            .any(|n| n.contains("left .collab/.c3/snapshot_seq")),
        "{notes:?}"
    );
    assert!(
        notes.iter().any(|n| n.contains("anchor, index")),
        "{notes:?}"
    );
    // the collab root IS the repository root: the same folder, nothing moves
    assert!(migrate_collab_cache(&r, &r).is_empty());
    // (point 11, e) a collab root inside the folder: `.c3/collab/.c3/*` moves to `.c3/*`
    let inner = path(&r).join("collab").join(STATE_DIR);
    std::fs::create_dir_all(inner.join("explains")).unwrap();
    let notes = migrate_collab_cache(&r, &path(&r).join("collab"));
    assert!(path(&r).join("explains").is_dir(), "{notes:?}");
    assert!(!inner.exists(), "the emptied source is removed");
    let _ = std::fs::remove_dir_all(&r);
}

// (S1 point 11) A work run whose owner is alive under the legacy folder blocks its move; a
// finished one does not.
#[test]
fn a_live_work_run_under_the_legacy_folder_is_found() {
    let r = scratch("live");
    let run_dir = legacy_path(&r).join("collab/t/work/01-zai-glm");
    std::fs::create_dir_all(&run_dir).unwrap();
    let mut rec = crate::work::record::RunRecord {
        id: "01-zai-glm".into(),
        task: "t".into(),
        seq: 1,
        status: "running".into(),
        started: "2026-11-23T10:00:00+00:00".into(),
        pid: Some(std::process::id() as i64),
        ..Default::default()
    };
    crate::work::record::write_record(&run_dir, &rec).unwrap();
    let why = live_run_under(&r, &legacy_path(&r)).expect("live");
    assert!(why.contains("a run is live under .eck/collab/t"), "{why}");
    rec.status = "done".into();
    crate::work::record::write_record(&run_dir, &rec).unwrap();
    assert_eq!(live_run_under(&r, &legacy_path(&r)), None);
    let _ = std::fs::remove_dir_all(&r);
}

// (S1 point 11) A path inside either folder is split into the folder and the rest; others are not.
#[test]
fn collab_paths_are_split_by_their_folder() {
    let r = scratch("split");
    assert_eq!(
        split_state_path(&r, &r.join(".eck").join("collab")),
        Some((StateName::Legacy, "collab".to_string()))
    );
    assert_eq!(
        split_state_path(&r, &r.join(".").join(".c3").join("collab").join("x")),
        Some((StateName::Current, "collab/x".to_string()))
    );
    assert_eq!(split_state_path(&r, &r.join(".collab")), None);
    assert_eq!(split_state_path(&r, Path::new("/elsewhere/.eck")), None);
    let _ = std::fs::remove_dir_all(&r);
}

// ------------------------------------------------------------------------------ S2

/// A directory link at `link` to `target`: a junction on Windows (no privilege needed), a symlink
/// elsewhere. `false` when the platform refused.
fn dir_link(link: &Path, target: &Path) -> bool {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
}

/// Remove a directory link made by [`dir_link`] (never its target).
fn drop_link(link: &Path) {
    #[cfg(windows)]
    let _ = std::fs::remove_dir(link);
    #[cfg(not(windows))]
    let _ = std::fs::remove_file(link);
}

/// Two paths name the same place (compared canonically: a CI runner's TEMP is an 8.3 short path).
fn same_place(a: &Path, b: &Path) -> bool {
    let c = |p: &Path| canonical_prefix(&lexical(p)).unwrap_or_else(|| p.to_path_buf());
    c(a) == c(b)
}

/// The old tool's folder as an upgrade finds it.
fn seed_eck(r: &Path) {
    let eck = legacy_path(r);
    std::fs::create_dir_all(eck.join("lastsnapshot")).unwrap();
    std::fs::create_dir_all(eck.join("collab").join("t")).unwrap();
    std::fs::write(eck.join("ROADMAP.md"), "# ROADMAP\n\nKEEP\n").unwrap();
    std::fs::write(eck.join("anchor"), "old-tool-anchor\n").unwrap();
    std::fs::write(eck.join("update_seq"), "7\n").unwrap();
    std::fs::write(eck.join("telemetry_queue.json"), "[]\n").unwrap();
    std::fs::write(
        eck.join("lastsnapshot").join("AnswerToSA.md"),
        "OLD-REPORT\n",
    )
    .unwrap();
}

// (F36-5) Every spelling of the legacy collab directory - relative with `.` or `..`, through `.c3`,
// absolute, another case where the file system folds case - is classified as `.eck/collab`.
#[test]
fn collab_paths_are_normalised_before_they_are_classified() {
    let r = scratch("normal");
    let legacy = Some((StateName::Legacy, "collab".to_string()));
    for spelling in [
        ".eck/collab",
        "./.eck/collab",
        "sub/../.eck/collab",
        ".c3/../.eck/collab",
        "./sub/./../.eck/./collab/",
    ] {
        assert_eq!(
            split_state_path(&r, &r.join(spelling)),
            legacy,
            "{spelling}"
        );
        assert_eq!(
            split_state_path(&r, Path::new(spelling)),
            legacy,
            "{spelling}"
        );
    }
    assert_eq!(
        split_state_path(&r, &r.join("sub").join("..").join(".eck").join("collab")),
        legacy
    );
    // a `..` that leaves the repository is not in it
    assert_eq!(split_state_path(&r, &r.join("../.eck/collab")), None);
    assert_eq!(
        split_state_path(&r, &r.join(".ECK/collab")).is_some(),
        cfg!(any(windows, target_os = "macos")),
        "the case rule of the file system"
    );
    let _ = std::fs::remove_dir_all(&r);
}

// (F36-5) Before and after the move every spelling resolves to the one folder that holds the records,
// and none of them - nor a writer's directory - creates a new `.eck/` after the move.
#[test]
fn every_collab_spelling_resolves_to_the_same_records_before_and_after_the_move() {
    let r = scratch("spellings");
    seed_eck(&r);
    let spellings = [
        ".eck/collab",
        "./.eck/collab",
        "sub/../.eck/collab",
        ".c3/../.eck/collab",
    ];
    for s in spellings {
        let got = collab_dir(&r, r.join(s));
        assert!(
            same_place(&got, &legacy_path(&r).join("collab")),
            "{s}: {got:?}"
        );
    }
    // the move, by hand
    std::fs::rename(legacy_path(&r), path(&r)).unwrap();
    for s in spellings {
        let got = collab_dir(&r, r.join(s));
        assert!(same_place(&got, &path(&r).join("collab")), "{s}: {got:?}");
        let dir = prepare_dir(&r, &r.join(s).join("t")).unwrap();
        assert!(same_place(&dir, &path(&r).join("collab").join("t")), "{s}");
        assert!(
            std::fs::symlink_metadata(legacy_path(&r)).is_err(),
            "{s} created a new .eck/"
        );
    }
    let _ = std::fs::remove_dir_all(&r);
}

// (F36-4) The default resolver never moves the folder: a `.c3/...` spelling on an unmoved repository
// reads the legacy path; only the moving resolver (a real consultation / work start) moves it.
#[test]
fn only_the_moving_resolver_moves_the_folder() {
    let r = scratch("readonly");
    seed_eck(&r);
    let got = collab_dir(&r, r.join(".c3").join("collab"));
    assert!(same_place(&got, &legacy_path(&r).join("collab")), "{got:?}");
    assert!(real_dir(&legacy_path(&r)), ".eck/ is still there");
    assert!(std::fs::symlink_metadata(path(&r)).is_err(), "no .c3/");

    let got = collab_dir_moving(&r, r.join(".c3").join("collab"));
    assert!(same_place(&got, &path(&r).join("collab")), "{got:?}");
    assert!(std::fs::symlink_metadata(legacy_path(&r)).is_err());
    assert!(path(&r).join("collab").join("t").is_dir());
    let _ = std::fs::remove_dir_all(&r);
}

// (F36-1) The move lock is exclusive, lives in the repository's git directory (a worktree's common
// dir), and a lock whose owner is gone is taken over.
#[test]
fn the_move_lock_is_exclusive_and_a_dead_owners_lock_is_taken_over() {
    let r = scratch("lock");
    std::fs::create_dir_all(r.join(".git")).unwrap();
    let first = MoveLock::acquire_within(&r, "test", Duration::from_millis(0)).unwrap();
    assert!(first.path.starts_with(r.join(".git")), "{:?}", first.path);
    let held = MoveLock::acquire_within(&r, "test", Duration::from_millis(200)).unwrap_err();
    assert!(held.contains("the move lock is held by c3 pid"), "{held}");
    let p = first.path.clone();
    drop(first);
    assert!(!p.exists(), "released on drop");
    // a record whose owner is gone (no such pid on this machine)
    let dead = LockRec {
        pid: u32::MAX - 7,
        start_time: String::new(),
        host: c3_core::host::machine_name(),
        created: chrono::Utc::now().to_rfc3339(),
        purpose: "move".into(),
        token: "dead".into(),
    };
    std::fs::write(&p, serde_json::to_vec(&dead).unwrap()).unwrap();
    let taken = MoveLock::acquire_within(&r, "test", Duration::from_millis(0)).unwrap();
    assert_eq!(read_lock(&p).unwrap().token, taken.token);
    drop(taken);

    // a worktree: `.git` is a file naming its git dir, whose `commondir` names the common one
    let wt = scratch("lock-wt");
    let gitdir = r.join(".git").join("worktrees").join("w");
    std::fs::create_dir_all(&gitdir).unwrap();
    std::fs::write(gitdir.join("commondir"), "../..\n").unwrap();
    std::fs::write(wt.join(".git"), format!("gitdir: {}\n", gitdir.display())).unwrap();
    let common = git_common_dir(&wt).unwrap();
    assert!(same_place(&common, &r.join(".git")), "{common:?}");
    let _ = std::fs::remove_dir_all(&wt);
    let _ = std::fs::remove_dir_all(&r);
}

// (F36-1) A writer that waits for the lock while another process moves the folder reads the state
// again once it has it: `.c3/` there, `.eck/` gone - it uses `.c3/` and creates no second folder.
#[test]
fn a_mover_that_waited_for_the_lock_reads_the_state_again() {
    let r = scratch("loser");
    seed_eck(&r);
    let held = MoveLock::acquire_within(&r, "test", Duration::from_millis(0)).unwrap();
    let r2 = r.clone();
    let loser = std::thread::spawn(move || {
        let dir = collab_dir_moving(&r2, r2.join(".c3").join("collab"));
        let m = migrate_legacy(&r2);
        (dir, m.deferred)
    });
    std::thread::sleep(Duration::from_millis(300));
    std::fs::rename(legacy_path(&r), path(&r)).unwrap();
    drop(held);
    let (dir, deferred) = loser.join().unwrap();
    assert!(same_place(&dir, &path(&r).join("collab")), "{dir:?}");
    assert_eq!(deferred, None);
    assert!(
        std::fs::symlink_metadata(legacy_path(&r)).is_err(),
        "no new .eck/"
    );
    let _ = std::fs::remove_dir_all(&r);
}

// (F36-6) A run that starts under the legacy folder while the mover holds the lock waits, then sees
// the folder moved and starts nothing (never a record under a new `.eck/`); a mover waits for a run
// that is creating its records.
#[test]
fn a_run_start_and_a_move_never_interleave() {
    let r = scratch("runstart");
    seed_eck(&r);
    let collab = legacy_path(&r).join("collab");
    let held = MoveLock::acquire_within(&r, "test", Duration::from_millis(0)).unwrap();
    let (r2, c2) = (r.clone(), collab.clone());
    let run = std::thread::spawn(move || begin_run(&r2, &c2).map(|_| ()));
    std::thread::sleep(Duration::from_millis(300));
    std::fs::rename(legacy_path(&r), path(&r)).unwrap();
    drop(held);
    let err = run.join().unwrap().unwrap_err();
    assert!(err.contains("moved from .eck/ to .c3/"), "{err}");
    assert!(std::fs::symlink_metadata(legacy_path(&r)).is_err());
    let _ = std::fs::remove_dir_all(&r);

    // the other way round: the run holds the lock while it creates its records
    let r = scratch("runstart2");
    seed_eck(&r);
    let collab = legacy_path(&r).join("collab");
    let start = begin_run(&r, &collab).unwrap();
    assert!(start._lock.is_some(), "a start under .eck/ takes the lock");
    let r2 = r.clone();
    let mover = std::thread::spawn(move || migrate_legacy(&r2));
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        real_dir(&legacy_path(&r)),
        "the mover waits for the run's records"
    );
    drop(start);
    let m = mover.join().unwrap();
    assert!(m.moved, "{:?}", m.notes);
    // a start outside the legacy folder takes nothing
    assert!(begin_run(&r, &r.join(".collab")).unwrap()._lock.is_none());
    let _ = std::fs::remove_dir_all(&r);
}

// (F36-2) The leftovers are relocated INSIDE `.eck/` before the rename, each step idempotent; a name
// that exists already keeps its bytes and the leftover goes to the first free `<name>.<n>`.
#[test]
fn the_leftovers_are_relocated_inside_the_old_folder_idempotently() {
    let r = scratch("leftovers");
    seed_eck(&r);
    let eck = legacy_path(&r);
    let rel = relocate_leftovers(&eck, LEGACY_STATE_DIR);
    assert_eq!(rel.blocked, None);
    assert!(rel.report);
    for n in LEGACY_LEFTOVERS {
        assert!(eck.join(LEGACY_SUBDIR).join(n).is_file(), "{n}");
        assert!(
            std::fs::symlink_metadata(eck.join(n)).is_err(),
            "{n} on top"
        );
    }
    assert_eq!(
        std::fs::read_to_string(eck.join(LAST_REPORT)).unwrap(),
        "OLD-REPORT\n"
    );
    // again: nothing left to do
    let again = relocate_leftovers(&eck, LEGACY_STATE_DIR);
    assert!(again.moved.is_empty() && !again.report && again.left.is_empty());
    // the old tool wrote a new anchor meanwhile: both are kept
    std::fs::write(eck.join("anchor"), "newer\n").unwrap();
    let third = relocate_leftovers(&eck, LEGACY_STATE_DIR);
    assert_eq!(third.moved, vec!["anchor".to_string()]);
    assert_eq!(
        std::fs::read_to_string(eck.join(LEGACY_SUBDIR).join("anchor")).unwrap(),
        "old-tool-anchor\n"
    );
    assert_eq!(
        std::fs::read_to_string(eck.join(LEGACY_SUBDIR).join("anchor.1")).unwrap(),
        "newer\n"
    );
    let _ = std::fs::remove_dir_all(&r);
}

// (F36-2) A `.c3/` an S1 build left half done (the old tool's `update_seq` at its top) is finished;
// a `.c3/` whose only `anchor` is C3's own is left alone.
#[test]
fn a_half_done_folder_is_finished_and_c3s_own_anchor_is_kept() {
    let r = scratch("halfdone");
    seed_eck(&r);
    std::fs::rename(legacy_path(&r), path(&r)).unwrap();
    let notes = finish_half_done(&r);
    let c3 = path(&r);
    for n in LEGACY_LEFTOVERS {
        assert!(c3.join(LEGACY_SUBDIR).join(n).is_file(), "{n}: {notes:?}");
        assert!(std::fs::symlink_metadata(c3.join(n)).is_err(), "{n}");
    }
    assert_eq!(
        std::fs::read_to_string(c3.join(LAST_REPORT)).unwrap(),
        "OLD-REPORT\n"
    );
    assert!(
        notes.iter().any(|n| n.starts_with("finished the move")),
        "{notes:?}"
    );
    let r2 = scratch("halfdone-own");
    std::fs::create_dir_all(path(&r2)).unwrap();
    std::fs::write(path(&r2).join("anchor"), "c3-own\n").unwrap();
    assert!(finish_half_done(&r2).is_empty());
    assert_eq!(
        std::fs::read_to_string(path(&r2).join("anchor")).unwrap(),
        "c3-own\n"
    );
    let _ = std::fs::remove_dir_all(&r);
    let _ = std::fs::remove_dir_all(&r2);
}

// (F36-3) Inside `.eck/`: a linked `legacy` stops the move (the anchor stays, the target is
// untouched); a linked `lastsnapshot` is never read through (its report stays where it is); a linked
// collab directory stops the move (the runs behind it cannot be checked).
#[test]
fn links_inside_the_old_folder_are_never_followed() {
    let r = scratch("links-legacy");
    seed_eck(&r);
    let outside = r.with_extension("outside");
    std::fs::create_dir_all(&outside).unwrap();
    if !dir_link(&legacy_path(&r).join(LEGACY_SUBDIR), &outside) {
        eprintln!("skipped: the platform made no directory link");
        return;
    }
    let m = migrate_legacy(&r);
    assert!(!m.moved);
    assert!(
        m.deferred
            .as_deref()
            .unwrap_or("")
            .contains("anchor could not be moved"),
        "{m:?}"
    );
    assert!(legacy_path(&r).join("anchor").is_file());
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    assert!(std::fs::symlink_metadata(path(&r)).is_err());
    drop_link(&legacy_path(&r).join(LEGACY_SUBDIR));

    // a linked `lastsnapshot`: the move goes on, the outside report is never taken
    let snap = legacy_path(&r).join("lastsnapshot");
    std::fs::remove_dir_all(&snap).unwrap();
    std::fs::write(outside.join("AnswerToSA.md"), "OUTSIDE\n").unwrap();
    assert!(dir_link(&snap, &outside));
    let m = migrate_legacy(&r);
    assert!(m.moved, "{m:?}");
    assert_eq!(
        std::fs::read_to_string(outside.join("AnswerToSA.md")).unwrap(),
        "OUTSIDE\n"
    );
    // (the report the first, stopped attempt relocated inside `.eck/` - never the outside one)
    assert_eq!(
        std::fs::read_to_string(path(&r).join(LAST_REPORT)).unwrap(),
        "OLD-REPORT\n"
    );
    assert!(
        m.notes.iter().any(|n| n.contains("left .c3/lastsnapshot")),
        "{:?}",
        m.notes
    );
    drop_link(&path(&r).join("lastsnapshot"));
    let _ = std::fs::remove_dir_all(&r);

    // a linked collab directory: the move waits
    let r = scratch("links-collab");
    seed_eck(&r);
    std::fs::remove_dir_all(legacy_path(&r).join("collab")).unwrap();
    std::fs::create_dir_all(outside.join("t")).unwrap();
    assert!(dir_link(&legacy_path(&r).join("collab"), &outside));
    let m = migrate_legacy(&r);
    assert!(!m.moved);
    assert!(
        m.deferred
            .as_deref()
            .unwrap_or("")
            .contains("the runs behind it are not checked"),
        "{m:?}"
    );
    assert!(
        legacy_path(&r).join("anchor").is_file(),
        "nothing relocated"
    );
    drop_link(&legacy_path(&r).join("collab"));
    let _ = std::fs::remove_dir_all(&r);
    let _ = std::fs::remove_dir_all(&outside);
}

// (F36-3) A write never goes through a link: a linked `.c3`, a linked directory below it; and the
// cache never moves out of a collab directory reached through a link.
#[test]
fn writes_and_the_cache_move_never_go_through_a_link() {
    let r = scratch("links-write");
    let outside = r.with_extension("outside");
    std::fs::create_dir_all(outside.join(STATE_DIR)).unwrap();
    std::fs::write(outside.join(STATE_DIR).join("anchor"), "cache\n").unwrap();
    std::fs::create_dir_all(path(&r)).unwrap();
    if !dir_link(&path(&r).join("scouts"), &outside) {
        eprintln!("skipped: the platform made no directory link");
        return;
    }
    let err = prepare_dir(&r, &path(&r).join("scouts")).unwrap_err();
    assert!(err.contains("never writes through a link"), "{err}");
    let err = write_in(&r, &path(&r).join("scouts"), "x.md", |_| Ok(b"x".to_vec())).unwrap_err();
    assert!(err.contains("never writes through a link"), "{err}");
    assert!(!outside.join("x.md").exists());
    // the cache under a linked collab directory stays where it is
    assert!(dir_link(&path(&r).join("collab"), &outside));
    let notes = migrate_collab_cache(&r, &path(&r).join("collab"));
    assert!(
        notes
            .iter()
            .any(|n| n.contains("the cache under it is not moved")),
        "{notes:?}"
    );
    assert!(outside.join(STATE_DIR).join("anchor").is_file());
    assert!(std::fs::symlink_metadata(path(&r).join("anchor")).is_err());
    drop_link(&path(&r).join("scouts"));
    drop_link(&path(&r).join("collab"));
    std::fs::remove_dir_all(path(&r)).unwrap();
    // `.c3` itself a link: refused, nothing written into its target, nothing created beside it
    assert!(dir_link(&path(&r), &outside));
    assert!(link_problem(&r)
        .unwrap()
        .contains("never writes through a link"));
    let err = ensure(&r, None).unwrap_err();
    assert!(err.contains("never writes through a link"), "{err}");
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 1);
    assert!(std::fs::symlink_metadata(legacy_path(&r)).is_err());
    assert_eq!(read_path(&r), None, "a linked .c3 is not read through");
    drop_link(&path(&r));
    let _ = std::fs::remove_dir_all(&r);
    let _ = std::fs::remove_dir_all(&outside);
}

// (F36-6) The live-run guard fails closed: a run record that cannot be read and a held ownership
// lock both count as a live run.
#[test]
fn the_live_run_guard_fails_closed() {
    let r = scratch("failclosed");
    let task = legacy_path(&r).join("collab").join("t");
    let run_dir = task.join("work").join("01-zai-glm");
    std::fs::create_dir_all(&run_dir).unwrap();
    std::fs::write(run_dir.join("run.json"), "{ not json").unwrap();
    let why = live_run_under(&r, &legacy_path(&r)).expect("unreadable record");
    assert!(why.contains("a run may be live"), "{why}");
    std::fs::remove_dir_all(task.join("work")).unwrap();
    assert_eq!(live_run_under(&r, &legacy_path(&r)), None);

    let slug = c3_core::task_slug::TaskSlug::new("t").unwrap();
    let lock =
        c3_core::store::take_task_lock_at(&task, &c3_core::store::LockRecord::now(&slug, None))
            .unwrap()
            .expect("lock taken");
    let why = live_run_under(&r, &legacy_path(&r)).expect("a held task lock");
    assert!(why.contains("a run is live under .eck/collab/t"), "{why}");
    drop(lock);
    assert_eq!(live_run_under(&r, &legacy_path(&r)), None);
    let _ = std::fs::remove_dir_all(&r);
}

// (F36-8) A `.c3/` or `.eck/` directory at any depth is kept out of a `--diff` patch.
#[test]
fn nested_state_folders_are_kept_out_of_a_diff_patch() {
    let r = scratch("diff");
    let run_git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(&r)
            .args(["-c", "user.email=t@example.com", "-c", "user.name=Test"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "git {args:?}");
    };
    run_git(&["init", "-q"]);
    std::fs::write(r.join("base.txt"), "base\n").unwrap();
    run_git(&["add", "-A"]);
    run_git(&["commit", "-qm", "base"]);
    let files = [
        ("pkg/.eck/x.txt", "NESTED-ECK"),
        ("pkg/.c3/y.txt", "NESTED-C3"),
        ("deep/er/.c3/z.txt", "DEEP-C3"),
        (".c3/top.txt", "TOP-C3"),
        ("pkg/keep.txt", "KEPT"),
    ];
    for (p, body) in files {
        let f = r.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, format!("{body}\n")).unwrap();
    }
    if cfg!(any(windows, target_os = "macos")) {
        std::fs::create_dir_all(r.join("lib").join(".ECK")).unwrap();
        std::fs::write(r.join("lib/.ECK/w.txt"), "UPPER-ECK\n").unwrap();
    }
    run_git(&["add", "-A", "-f"]);
    run_git(&["commit", "-qm", "state folders"]);
    let ex = crate::consult::reviewed::diff_excludes(&r, &r.join(".collab"));
    let spec = crate::consult::reviewed::check_diff_spec(&r, "HEAD~1..HEAD").unwrap();
    let patch =
        String::from_utf8(crate::consult::reviewed::render_patch(&r, &spec, &ex).unwrap()).unwrap();
    assert!(patch.contains("+KEPT"), "{patch}");
    for gone in ["NESTED-ECK", "NESTED-C3", "DEEP-C3", "TOP-C3", "UPPER-ECK"] {
        assert!(!patch.contains(gone), "{gone} leaked: {patch}");
    }
    let _ = std::fs::remove_dir_all(&r);
}
