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
    // (R7b, F47-3) trailing dots and spaces name the same entry on Windows - and another one
    // elsewhere
    for spelling in [
        ".eck./collab",
        ".eck /collab",
        ".eck. ./collab",
        ".eck/collab.",
    ] {
        let want = if cfg!(windows) {
            legacy.clone()
        } else if spelling.starts_with(".eck/") {
            Some((StateName::Legacy, "collab.".to_string()))
        } else {
            None
        };
        assert_eq!(split_state_path(&r, &r.join(spelling)), want, "{spelling}");
        assert_eq!(
            split_state_path(&r, Path::new(spelling)),
            want,
            "{spelling}"
        );
    }
    assert_eq!(
        split_state_path(&r, Path::new(".c3 /collab")),
        if cfg!(windows) {
            Some((StateName::Current, "collab".to_string()))
        } else {
            None
        }
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
    // (the acceptance review) the compare-and-remove of a takeover is one step under the gate:
    // while another taker holds the gate nothing is removed, so a taker can never remove the lock
    // another one created after the stale record it judged
    let gate = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(gate_path(&p))
        .unwrap();
    gate.lock().unwrap();
    assert!(
        !take_over_if_stale(&p),
        "nothing is taken while the gate is held"
    );
    assert_eq!(
        read_lock(&p).unwrap().token,
        "dead",
        "the judged record is still there"
    );
    drop(gate);
    let taken = MoveLock::acquire_within(&r, "test", Duration::from_millis(0)).unwrap();
    assert_eq!(read_lock(&p).unwrap().token, taken.token);
    // a taker that judged the dead record before `taken` replaced it removes nothing now
    assert!(
        !take_over_if_stale(&p),
        "a live owner's lock is not taken over"
    );
    assert_eq!(read_lock(&p).unwrap().token, taken.token);
    drop(taken);
    assert!(!p.exists(), "released on drop, under the gate");
    assert!(gate_path(&p).exists(), "the gate file is never removed");

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

// (0.2.5, acceptance F57-4 / RC3) A creator of the move lock paused right after its exclusive
// create - inside the gate now - for longer than the garbage interval, while a second acquirer runs
// with that interval shortened to nothing: the creator's still-empty file is never taken over, at
// most one of them ever holds the lock (each holds it until the creator was released, so two owners
// would overlap), and the paused creator completes as the owner; the second gets the lock only once
// the creator released it.
#[test]
fn a_lock_creator_paused_after_its_exclusive_create_is_never_a_second_owner() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    let r = scratch("lock-create");
    std::fs::create_dir_all(r.join(".git")).unwrap();
    let p = lock_path(&r);
    let release = r.with_extension("release");
    let mut held = release.clone().into_os_string();
    held.push(".held");
    let held = PathBuf::from(held);
    let inside = Arc::new(AtomicUsize::new(0));
    let most = Arc::new(AtomicUsize::new(0));
    // the protected section: count who is in it; stay until the creator was released
    let section = {
        let (inside, most, release) = (inside.clone(), most.clone(), release.clone());
        move || {
            let n = inside.fetch_add(1, Ordering::SeqCst) + 1;
            most.fetch_max(n, Ordering::SeqCst);
            let deadline = Instant::now() + Duration::from_secs(30);
            while !release.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            std::thread::sleep(Duration::from_millis(700));
            inside.fetch_sub(1, Ordering::SeqCst);
        }
    };
    let (r1, rel1, s1) = (r.clone(), release.clone(), section.clone());
    let creator = std::thread::spawn(move || {
        let _h =
            c3_core::test_hooks::set_thread_hook(LOCK_CREATE_HOLD_HOOK, &rel1.to_string_lossy());
        let got = MoveLock::acquire_within(&r1, "paused creator", Duration::from_secs(30));
        if got.is_ok() {
            s1();
        }
        got.map(|l| l.token.clone())
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    while !held.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        held.exists(),
        "the creator never reached its exclusive create"
    );
    // the creator's file is there, still empty, and grows older than the garbage interval
    assert_eq!(std::fs::read(&p).unwrap().len(), 0);
    std::thread::sleep(Duration::from_millis(1500));
    let (r2, s2) = (r.clone(), section.clone());
    let second = std::thread::spawn(move || {
        let _h = c3_core::test_hooks::set_thread_hook(LOCK_GARBAGE_HOOK, "0");
        let got = MoveLock::acquire_within(&r2, "second", Duration::from_secs(20));
        if got.is_ok() {
            s2();
        }
        got.map(|l| l.token.clone())
    });
    // the second acquirer tries meanwhile: the creator's empty file stays, nobody holds the lock
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(
        std::fs::read(&p).unwrap().len(),
        0,
        "the creator's empty file was taken over"
    );
    assert_eq!(
        inside.load(Ordering::SeqCst),
        0,
        "an owner while the creator is paused"
    );
    std::fs::write(&release, "go").unwrap();
    let first = creator.join().unwrap();
    let second = second.join().unwrap();
    assert!(first.is_ok(), "the paused creator is the owner: {first:?}");
    assert!(
        second.is_ok(),
        "the second gets the lock after it: {second:?}"
    );
    assert_ne!(first, second);
    assert_eq!(most.load(Ordering::SeqCst), 1, "never two owners at once");
    assert!(!p.exists(), "both released");
    let _ = std::fs::remove_file(&release);
    let _ = std::fs::remove_file(&held);
    let _ = std::fs::remove_dir_all(&r);
}

// (0.2.5, acceptance answer 2) Beside a real `.eck/` a fresh `.c3/` is created only under the move
// lock - by every writer, whatever the migration switch: while another process holds the lock no
// writer creates it (a writer's directory and a run start refuse, the cache move leaves its
// entries, `ensure` goes on in `.eck/` and says so - its switch-off twin is the c3-cli test
// `a_writer_with_the_switch_off_never_splits_a_move`); with the lock free it is created, fresh and
// marked. Without a `.eck/` no lock is needed.
#[test]
fn a_fresh_folder_beside_the_old_one_is_created_only_under_the_move_lock() {
    let r = scratch("create-locked");
    std::fs::create_dir_all(r.join(".git")).unwrap();
    seed_eck(&r);
    let collab = r.join(".collab");
    std::fs::create_dir_all(collab.join(STATE_DIR)).unwrap();
    std::fs::write(collab.join(STATE_DIR).join("index-entry"), "x\n").unwrap();
    let held = MoveLock::acquire_within(&r, "test", Duration::from_millis(0)).unwrap();
    {
        let _h = c3_core::test_hooks::set_thread_hook(LOCK_WAIT_HOOK, "100");
        let root = Root::open_anchor(&r).unwrap();
        assert!(matches!(state_folder(&r, &root), Err(FolderError::Held(_))));
        let err = prepare_dir(&r, &path(&r).join("explains")).unwrap_err();
        assert!(err.contains(".c3/ could not be created now"), "{err}");
        let err = begin_run(&r, &path(&r).join("collab"), "t", &["handoffs"]).unwrap_err();
        assert!(
            err.contains(".c3/ could not be created now") && err.contains("nothing was started"),
            "{err}"
        );
        let notes = migrate_collab_cache(&r, &collab);
        assert!(
            notes
                .iter()
                .any(|n| n.contains("not moved: .c3/ could not be created now")),
            "{notes:?}"
        );
        assert!(
            std::fs::symlink_metadata(path(&r)).is_err(),
            "no writer created .c3/ while the lock was held"
        );
    }
    drop(held);
    let root = Root::open_anchor(&r).unwrap();
    let (_d, fresh) = state_folder(&r, &root)
        .map_err(|e| format!("{e:?}"))
        .unwrap();
    assert!(fresh && is_finished(&path(&r)), "fresh and marked");
    assert!(real_dir(&legacy_path(&r)), ".eck/ is left as it is");
    // no `.eck/`: no lock is needed
    let r2 = scratch("create-free");
    std::fs::create_dir_all(r2.join(".git")).unwrap();
    let held = MoveLock::acquire_within(&r2, "test", Duration::from_millis(0)).unwrap();
    let root = Root::open_anchor(&r2).unwrap();
    assert!(state_folder(&r2, &root).is_ok_and(|(_, fresh)| fresh));
    drop(held);
    let _ = std::fs::remove_dir_all(&r);
    let _ = std::fs::remove_dir_all(&r2);
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
    let run = std::thread::spawn(move || begin_run(&r2, &c2, "t", &["handoffs"]).map(|_| ()));
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
    let start = begin_run(&r, &collab, "t", &["handoffs"]).unwrap();
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
    assert!(begin_run(&r, &r.join(".collab"), "t", &["handoffs"])
        .unwrap()
        ._lock
        .is_none());
    let _ = std::fs::remove_dir_all(&r);
}

// (F36-2) The leftovers are relocated INSIDE `.eck/` before the rename, each step idempotent; a name
// that exists already keeps its bytes and the leftover goes to the first free `<name>.<n>`.
#[test]
fn the_leftovers_are_relocated_inside_the_old_folder_idempotently() {
    let r = scratch("leftovers");
    seed_eck(&r);
    let eck = legacy_path(&r);
    let rel = relocate_leftovers(&handle(&eck), LEGACY_STATE_DIR);
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
    let again = relocate_leftovers(&handle(&eck), LEGACY_STATE_DIR);
    assert!(again.moved.is_empty() && !again.report && again.left.is_empty());
    // the old tool wrote a new anchor meanwhile: both are kept
    std::fs::write(eck.join("anchor"), "newer\n").unwrap();
    let third = relocate_leftovers(&handle(&eck), LEGACY_STATE_DIR);
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
// (R7b, F48-3) a `.c3/` marked finished keeps its `anchor` - C3's own.
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
    assert!(is_finished(&c3), "the finished folder is marked");
    let r2 = scratch("halfdone-own");
    std::fs::create_dir_all(path(&r2)).unwrap();
    std::fs::write(path(&r2).join(LAYOUT_MARKER), LAYOUT_BODY).unwrap();
    std::fs::write(path(&r2).join("anchor"), "c3-own\n").unwrap();
    assert!(finish_half_done(&r2).is_empty());
    assert_eq!(
        std::fs::read_to_string(path(&r2).join("anchor")).unwrap(),
        "c3-own\n"
    );
    let _ = std::fs::remove_dir_all(&r);
    let _ = std::fs::remove_dir_all(&r2);
}

// (R7b, F48-3) A `.c3/` holding only the old tool's `anchor` and no marker is unfinished: no anchor
// is read from it, the next writing command sends that `anchor` to `legacy/` and marks the folder;
// a cache `anchor` moved in afterwards (C3's own) stays and is read. A fresh `.c3/` is marked at
// once.
#[test]
fn a_folder_without_its_marker_sends_a_lone_anchor_to_legacy() {
    let r = scratch("lone-anchor");
    let c3 = path(&r);
    std::fs::create_dir_all(&c3).unwrap();
    std::fs::write(c3.join("anchor"), "old-tool-anchor\n").unwrap();
    assert_eq!(crate::pack::snapshot::read_anchor(&c3), None, "no marker");
    let collab = r.join(".collab");
    std::fs::create_dir_all(collab.join(STATE_DIR)).unwrap();
    std::fs::write(collab.join(STATE_DIR).join("anchor"), "c3-cache-anchor\n").unwrap();
    let e = ensure(&r, Some(&collab)).unwrap();
    assert!(!e.new_folder);
    assert_eq!(
        std::fs::read_to_string(c3.join(LEGACY_SUBDIR).join("anchor")).unwrap(),
        "old-tool-anchor\n",
        "{:?}",
        e.moved
    );
    assert!(is_finished(&c3));
    assert_eq!(
        crate::pack::snapshot::read_anchor(&c3).as_deref(),
        Some("c3-cache-anchor"),
        "{:?}",
        e.moved
    );
    // nothing more to finish: C3's own anchor stays
    assert!(finish_half_done(&r).is_empty());
    assert!(c3.join("anchor").is_file());
    // a fresh folder carries the marker from its creation
    let r2 = scratch("fresh-marker");
    assert!(ensure(&r2, None).unwrap().new_folder);
    assert!(is_finished(&path(&r2)));
    let _ = std::fs::remove_dir_all(&r);
    let _ = std::fs::remove_dir_all(&r2);
}

/// A handle on the directory `p` (a test opens the folder `relocate_leftovers` works in).
fn handle(p: &Path) -> Dir {
    Dir::open_ambient_dir(p, cap_std::ambient_authority()).unwrap()
}

// (R7b, F47-2) `legacy/` inside `.eck/` swapped for a link between its check and the relocation
// (`state-relocate`): no leftover reaches the link's target. On Windows the swap is refused while C3
// holds the directory's handle; elsewhere the leftovers land in the directory that was opened.
#[test]
fn a_directory_in_the_old_folder_swapped_for_a_link_takes_nothing_outside() {
    let r = scratch("race-relocate");
    seed_eck(&r);
    let outside = r.with_extension("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let legacy = legacy_path(&r).join(LEGACY_SUBDIR);
    let hook = c3_core::test_hooks::set_thread_hook(
        contain::RACE_HOOK,
        &format!("state-relocate|{}|{}", legacy.display(), outside.display()),
    );
    let m = migrate_legacy(&r);
    drop(hook);
    assert!(
        std::fs::read_dir(&outside).unwrap().next().is_none(),
        "nothing outside: {m:?}"
    );
    let folder = if m.moved { path(&r) } else { legacy_path(&r) };
    assert!(
        std::fs::symlink_metadata(folder.join("anchor")).is_err(),
        "{m:?}"
    );
    let kept = [LEGACY_SUBDIR, "legacy.c3-race-orig"]
        .iter()
        .any(|d| folder.join(d).join("anchor").is_file());
    assert!(
        kept,
        "the anchor is in the directory that was opened: {m:?}"
    );
    drop_link(&folder.join(LEGACY_SUBDIR));
    let _ = std::fs::remove_dir_all(&r);
    let _ = std::fs::remove_dir_all(&outside);
}

// (R7b, F47-2) `.c3` swapped for a link between `ensure` and a write (`state-write-create`, after
// the write's own checks): nothing is written outside - on Windows the swap is refused while C3
// holds the folder's handle, elsewhere the file lands in the directory that was opened.
#[test]
fn the_folder_swapped_for_a_link_before_a_write_never_writes_outside() {
    let r = scratch("race-write");
    let outside = r.with_extension("outside");
    std::fs::create_dir_all(&outside).unwrap();
    ensure(&r, None).unwrap();
    let c3 = path(&r);
    let hook = c3_core::test_hooks::set_thread_hook(
        contain::RACE_HOOK,
        &format!("state-write-create|{}|{}", c3.display(), outside.display()),
    );
    let w = write_in(&r, &c3, "JOURNAL.md", |_, _| Ok(b"entry\n".to_vec()));
    drop(hook);
    assert!(
        std::fs::read_dir(&outside).unwrap().next().is_none(),
        "nothing outside: {w:?}"
    );
    let aside = r.join(".c3.c3-race-orig");
    let landed = if aside.exists() { aside } else { c3.clone() };
    assert_eq!(
        std::fs::read_to_string(landed.join("JOURNAL.md")).unwrap(),
        "entry\n",
        "{w:?}"
    );
    if r.join(".c3.c3-race-orig").exists() {
        drop_link(&c3);
    }
    let _ = std::fs::remove_dir_all(&r);
    let _ = std::fs::remove_dir_all(&outside);
}

// (R7b, F48-1) `--collab-dir .c3/collab` (and `.eck/collab`) with the folder a junction (a symlink
// elsewhere): the run start refuses with one line, before and after the path was resolved through
// the link, and nothing is created in the target; a reader does not read through a linked `.eck`.
#[test]
fn a_collab_directory_under_a_linked_folder_refuses_the_run_and_creates_nothing() {
    let r = scratch("linked-collab");
    let outside = r.with_extension("outside");
    std::fs::create_dir_all(&outside).unwrap();
    if !dir_link(&path(&r), &outside) {
        eprintln!("skipped: the platform made no directory link");
        return;
    }
    let resolved = crate::providers::resolve_collab_root(&r, ".c3/collab");
    for given in [r.join(".c3").join("collab"), resolved] {
        let err = begin_run(&r, &given, "t", &["handoffs"]).unwrap_err();
        assert!(
            err.contains(".c3 is a link") && err.contains("nothing was started"),
            "{err}"
        );
    }
    std::fs::create_dir_all(outside.join("collab")).unwrap();
    let err = begin_run(&r, &outside.join("collab"), "t", &["handoffs"]).unwrap_err();
    assert!(
        err.contains("never reads or writes through a link"),
        "{err}"
    );
    assert_eq!(
        std::fs::read_dir(&outside).unwrap().count(),
        1,
        "only the test's own"
    );
    drop_link(&path(&r));
    // a linked `.eck`: refused for a writer, not read by a reader
    assert!(dir_link(&legacy_path(&r), &outside));
    assert!(begin_run(&r, &legacy_path(&r).join("collab"), "t", &["handoffs"]).is_err());
    assert_eq!(read_path(&r), None);
    drop_link(&legacy_path(&r));
    let _ = std::fs::remove_dir_all(&r);
    let _ = std::fs::remove_dir_all(&outside);
}

// (0.2.5, acceptance F57-6 / RC5) Inside C3's folder a task directory - or its `handoffs/` or
// `work/` - that is a link (a junction on Windows, a symlink elsewhere) refuses the run before
// anything is written, in one line that names it: for a run start under `.c3/collab` and (under the
// move lock, released again) `.eck/collab`, and for a panel member; nothing appears at the link's
// target. A normal start makes both directories. Outside the folder the same holds for what lies
// below the collab root: under the default `.collab/` every component from the repository root is
// opened without following it; a collab root outside the repository is the operator's own anchor
// and is followed itself, a linked task directory below it is not.
#[test]
fn a_linked_task_directory_inside_the_folder_refuses_the_run_and_creates_nothing() {
    let r = scratch("linked-task");
    let outside = r.with_extension("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let collab = path(&r).join("collab");
    std::fs::create_dir_all(&collab).unwrap();
    let t = collab.join("t");
    if !dir_link(&t, &outside) {
        eprintln!("skipped: the platform made no directory link");
        return;
    }
    let empty = |why: &str| {
        assert!(
            std::fs::read_dir(&outside).unwrap().next().is_none(),
            "something was created at the link's target: {why}"
        )
    };
    for dirs in [&["handoffs"][..], &["work"][..]] {
        let err = begin_run(&r, &collab, "t", dirs).unwrap_err();
        assert!(
            err.contains(".c3/collab/t is a link")
                && err.contains("through a link")
                && err.contains("nothing was started"),
            "{err}"
        );
        empty(&err);
    }
    // a panel member checks its panel's directories again
    let err = run_dirs(&r, &collab, "t", &["handoffs"]).unwrap_err();
    assert!(err.contains(".c3/collab/t is a link"), "{err}");
    empty(&err);
    drop_link(&t);
    // `handoffs/` (or `work/`) the link
    std::fs::create_dir_all(&t).unwrap();
    for name in ["handoffs", "work"] {
        assert!(dir_link(&t.join(name), &outside));
        let err = begin_run(&r, &collab, "t", &[name]).unwrap_err();
        assert!(
            err.contains(&format!(".c3/collab/t/{name} is a link")),
            "{err}"
        );
        empty(&err);
        drop_link(&t.join(name));
    }
    // a normal start makes both directories, real ones
    let s = begin_run(&r, &collab, "n", &["handoffs"]).unwrap();
    assert!(s._lock.is_none());
    assert!(real_dir(&collab.join("n")) && real_dir(&collab.join("n").join("handoffs")));
    // under the legacy folder: the same refusal, under the move lock
    let r2 = scratch("linked-task-eck");
    seed_eck(&r2);
    let ecollab = legacy_path(&r2).join("collab");
    assert!(dir_link(&ecollab.join("x"), &outside));
    let err = begin_run(&r2, &ecollab, "x", &["handoffs"]).unwrap_err();
    assert!(err.contains(".eck/collab/x is a link"), "{err}");
    empty(&err);
    assert!(!lock_path(&r2).exists(), "the move lock was released");
    drop_link(&ecollab.join("x"));
    // outside the folder, inside the repository (the default `.collab/`): a linked task
    // directory refuses too - it is tracked content and can arrive with a clone
    let c = r.join(".collab");
    std::fs::create_dir_all(&c).unwrap();
    assert!(dir_link(&c.join("t"), &outside));
    let err = begin_run(&r, &c, "t", &["handoffs"]).unwrap_err();
    assert!(
        err.contains(".collab/t is a link") && err.contains("through a link"),
        "{err}"
    );
    empty(&err);
    drop_link(&c.join("t"));
    // ... `handoffs/` the link
    std::fs::create_dir_all(c.join("t")).unwrap();
    assert!(dir_link(&c.join("t").join("handoffs"), &outside));
    let err = begin_run(&r, &c, "t", &["handoffs"]).unwrap_err();
    assert!(err.contains(".collab/t/handoffs is a link"), "{err}");
    empty(&err);
    drop_link(&c.join("t").join("handoffs"));
    // ... and the default collab root itself a link
    let c2 = r.join(".collab2");
    assert!(dir_link(&c2, &outside));
    let err = begin_run(&r, &c2, "t", &["handoffs"]).unwrap_err();
    assert!(err.contains(".collab2 is a link"), "{err}");
    empty(&err);
    // (F60-1) the commands' resolver keeps the link as it is named - its canonical form would
    // read as a directory outside the repository, the operator's own anchor, and be followed
    let resolved = crate::providers::resolve_collab_root(&r, ".collab2");
    assert_eq!(resolved, r.join(".collab2"), "the link is not resolved");
    let err = begin_run(&r, &resolved, "t", &["handoffs"]).unwrap_err();
    assert!(err.contains(".collab2 is a link"), "{err}");
    empty(&err);
    // ... the same for a collab root that is a link inside a real `.c3`
    let inner = path(&r).join("collab3");
    assert!(dir_link(&inner, &outside));
    let resolved = crate::providers::resolve_collab_root(&r, ".c3/collab3");
    assert_eq!(resolved, inner, "the link is not resolved");
    let err = begin_run(&r, &resolved, "t", &["handoffs"]).unwrap_err();
    assert!(err.contains(".c3/collab3 is a link"), "{err}");
    empty(&err);
    drop_link(&inner);
    // (F60-2) nor does a spelling that is not a textual prefix of the repository root decide it:
    // the repository reached through a link ABOVE its root ...
    let alias = r.with_extension("alias");
    assert!(dir_link(&alias, &r));
    let err = begin_run(&r, &alias.join(".collab2"), "t", &["handoffs"]).unwrap_err();
    assert!(err.contains(".collab2 is a link"), "{err}");
    empty(&err);
    assert_eq!(
        crate::providers::resolve_collab_root(&r, &alias.join(".collab2").to_string_lossy()),
        r.join(".collab2")
    );
    drop_link(&alias);
    // ... and a verbatim path
    #[cfg(windows)]
    {
        let verbatim = PathBuf::from(format!(r"\\?\{}", c2.display()));
        let err = begin_run(&r, &verbatim, "t", &["handoffs"]).unwrap_err();
        assert!(err.contains(".collab2 is a link"), "{err}");
        empty(&err);
    }
    drop_link(&c2);
    // (F62-2, RC2) `..` after a link is resolved by TEXT, in the resolver as in the check:
    // `l/../.collab4` is the repository's `.collab4`, never `<the target of l>/../.collab4`
    // (where the file system would take it when it applies `..` to the link's target)
    let side = r.with_extension("side");
    std::fs::create_dir_all(side.join("sub")).unwrap();
    std::fs::create_dir_all(side.join(".collab4")).unwrap();
    assert!(dir_link(&r.join("l"), &side.join("sub")));
    let resolved = crate::providers::resolve_collab_root(&r, "l/../.collab4");
    assert_eq!(resolved, r.join(".collab4"), "one spelling, by text");
    begin_run(&r, &resolved, "t", &["handoffs"]).unwrap();
    assert!(real_dir(&r.join(".collab4").join("t").join("handoffs")));
    assert!(
        std::fs::read_dir(side.join(".collab4"))
            .unwrap()
            .next()
            .is_none(),
        "nothing beside the link's target"
    );
    // the unresolved spelling handed straight to the run start names the same directory
    begin_run(
        &r,
        &r.join("l").join("..").join(".collab4"),
        "u",
        &["handoffs"],
    )
    .unwrap();
    assert!(real_dir(&r.join(".collab4").join("u").join("handoffs")));
    assert!(std::fs::read_dir(side.join(".collab4"))
        .unwrap()
        .next()
        .is_none());
    drop_link(&r.join("l"));
    let _ = std::fs::remove_dir_all(&side);
    // a real collab root is resolved as before: canonical
    let real = crate::providers::resolve_collab_root(&r, ".collab");
    assert_eq!(
        real,
        std::fs::canonicalize(r.join(".collab"))
            .map(crate::providers::dunce_simplify)
            .unwrap()
    );
    // a normal start under `.collab/` makes real directories
    begin_run(&r, &c, "n", &["handoffs"]).unwrap();
    assert!(real_dir(&c.join("n")) && real_dir(&c.join("n").join("handoffs")));
    // a collab root OUTSIDE the repository is the operator's own anchor: reached through a link
    // it is followed; a linked task directory below it is not
    let far = r.with_extension("far");
    let anchor = r.with_extension("anchor");
    std::fs::create_dir_all(&far).unwrap();
    assert!(dir_link(&anchor, &far));
    begin_run(&r, &anchor, "t", &["handoffs"]).unwrap();
    assert!(
        real_dir(&far.join("t").join("handoffs")),
        "the anchor itself is followed"
    );
    assert!(dir_link(&far.join("x"), &outside));
    let err = begin_run(&r, &anchor, "x", &["handoffs"]).unwrap_err();
    assert!(
        err.contains("is a link") && err.contains("through a link"),
        "{err}"
    );
    empty(&err);
    drop_link(&far.join("x"));
    drop_link(&anchor);
    let _ = std::fs::remove_dir_all(&far);
    let _ = std::fs::remove_dir_all(&r);
    let _ = std::fs::remove_dir_all(&r2);
    let _ = std::fs::remove_dir_all(&outside);
}

// (R7b, F48-2) A writer that lost the lock to a mover held inside the move (the hold hook) goes on
// with `.eck/` for its run; once the mover renamed, its write lands in `.c3/` and no `.eck/` is
// created again.
#[test]
fn a_writer_that_lost_the_lock_ends_in_the_new_folder_and_no_old_folder_returns() {
    let r = scratch("lost-lock");
    seed_eck(&r);
    let release = r.with_extension("release");
    let mut held = release.clone().into_os_string();
    held.push(".held");
    let held = PathBuf::from(held);
    let (r2, rel2) = (r.clone(), release.clone());
    let mover = std::thread::spawn(move || {
        let _h = c3_core::test_hooks::set_thread_hook(MOVE_HOLD_HOOK, &rel2.to_string_lossy());
        migrate_legacy(&r2)
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    while !held.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(held.exists(), "the mover never reached the hold");
    let ensured = {
        let _h = c3_core::test_hooks::set_thread_hook(LOCK_WAIT_HOOK, "100");
        ensure(&r, None).unwrap()
    };
    assert!(ensured.deferred.is_some(), "{:?}", ensured.moved);
    assert!(same_place(&ensured.dir, &legacy_path(&r)));
    std::fs::write(&release, "go").unwrap();
    let m = mover.join().unwrap();
    assert!(m.moved, "{m:?}");
    let f = write_in(&r, &ensured.dir, LAST_REPORT, |_, _| {
        Ok(b"report\n".to_vec())
    })
    .unwrap();
    assert!(same_place(&f, &path(&r).join(LAST_REPORT)), "{f:?}");
    let d = prepare_dir(&r, &ensured.dir.join("explains")).unwrap();
    assert!(same_place(&d, &path(&r).join("explains")), "{d:?}");
    assert!(
        std::fs::symlink_metadata(legacy_path(&r)).is_err(),
        "no new .eck/"
    );
    let _ = std::fs::remove_file(&release);
    let _ = std::fs::remove_file(&held);
    let _ = std::fs::remove_dir_all(&r);
}

// (R7b, F48-2) The move lock of an owner alive on this machine is not taken over, however old; a
// dead owner's is, and another host's once it is old. (0.2.5 D5) The command's line and `c3
// doctor`'s line for the folder name the owner: its pid, its host when it is another host's lock,
// and since when it holds the lock.
#[test]
fn a_live_owners_old_lock_is_not_taken_over() {
    let r = scratch("live-lock");
    std::fs::create_dir_all(r.join(".git")).unwrap();
    seed_eck(&r);
    let p = lock_path(&r);
    let me = std::process::id();
    let write_old = |rec: &LockRec| {
        std::fs::write(&p, serde_json::to_vec(rec).unwrap()).unwrap();
        let f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
        let old = std::time::SystemTime::now() - Duration::from_secs(LOCK_STALE_SECS + 120);
        f.set_modified(old).unwrap();
    };
    let live = LockRec {
        pid: me,
        start_time: crate::liveness::proc::process_start_iso(me).unwrap_or_default(),
        host: c3_core::host::machine_name(),
        created: "2026-01-01T00:00:00Z".into(),
        purpose: "move".into(),
        token: "live".into(),
    };
    write_old(&live);
    let held = MoveLock::acquire_within(&r, "test", Duration::from_millis(150)).unwrap_err();
    assert_eq!(
        held,
        format!("the move lock is held by c3 pid {me} (move) since 2026-01-01 00:00:00 UTC")
    );
    assert_eq!(read_lock(&p).unwrap().token, "live");
    // the line the command prints, and doctor's
    let ensured = {
        let _h = c3_core::test_hooks::set_thread_hook(LOCK_WAIT_HOOK, "100");
        ensure(&r, None).unwrap()
    };
    assert_eq!(ensured.deferred.as_deref(), Some(held.as_str()));
    assert!(
        ensured.moved.iter().any(|l| l
            == &format!(
                "the project folder could not be moved now ({held}); using {LEGACY_STATE_DIR}/ for this run"
            )),
        "{:?}",
        ensured.moved
    );
    assert_eq!(move_lock_holder(&r), Some(held.clone()));
    assert_eq!(read_lock(&p).unwrap().token, "live");
    // another host's FRESH lock: waited for, and its host is named (its text shown plain)
    let foreign = LockRec {
        host: "some-other-host\u{202e}".into(),
        created: "2026-11-24T10:20:30.123456789+02:00".into(),
        token: "foreign".into(),
        ..live.clone()
    };
    std::fs::write(&p, serde_json::to_vec(&foreign).unwrap()).unwrap();
    let held = MoveLock::acquire_within(&r, "test", Duration::from_millis(0)).unwrap_err();
    assert_eq!(
        held,
        format!(
            "the move lock is held by c3 pid {me} on host some-other-host? (move) since 2026-11-24 08:20:30 UTC"
        )
    );
    assert_eq!(move_lock_holder(&r), Some(held));
    // another host's old lock: its owner cannot be shown alive
    write_old(&LockRec {
        host: "some-other-host".into(),
        token: "foreign".into(),
        ..live.clone()
    });
    assert_eq!(move_lock_holder(&r), None, "a stale lock names no holder");
    let taken = MoveLock::acquire_within(&r, "test", Duration::from_millis(0)).unwrap();
    drop(taken);
    assert_eq!(move_lock_holder(&r), None);
    let _ = std::fs::remove_dir_all(&r);
}

// (0.2.5 D5) A write into a directory removes `write_in`'s own temporary files there that are older
// than a day (a writer killed between its write and its rename left them); a fresh one, a foreign
// file with a similar name and a directory of that name stay.
#[test]
fn write_in_sweeps_its_own_stale_temp_files() {
    let r = scratch("sweep");
    ensure(&r, None).unwrap();
    let c3 = path(&r);
    let old = std::time::SystemTime::now() - Duration::from_secs(WRITE_TEMP_STALE_SECS + 120);
    let stale = c3.join(write_temp_name(
        "JOURNAL.md",
        4242,
        1_700_000_000_123_456_789,
    ));
    let stale_other = c3.join(".ROADMAP.md.c3-7-8.tmp");
    let fresh = c3.join(".JOURNAL.md.c3-4243-1700000000123456790.tmp");
    let foreign = [
        "JOURNAL.md.c3-4242-1.tmp",
        ".JOURNAL.md.c3-pid-1.tmp",
        ".JOURNAL.md.c3-4242.tmp",
        ".JOURNAL.md.c3-4242-1.tmp.bak",
        "..c3-4242-1.tmp",
    ];
    for f in [&stale, &stale_other, &fresh] {
        std::fs::write(f, "partial").unwrap();
    }
    for f in foreign {
        std::fs::write(c3.join(f), "keep").unwrap();
    }
    let dir_named = c3.join(".x.md.c3-1-2.tmp");
    std::fs::create_dir_all(&dir_named).unwrap();
    // (a directory is opened for its times with backup semantics on Windows, read-only elsewhere)
    let age = |p: &Path| {
        #[cfg(windows)]
        let f = {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .access_mode(0x100) // FILE_WRITE_ATTRIBUTES
                .custom_flags(0x0200_0000) // FILE_FLAG_BACKUP_SEMANTICS
                .open(p)
                .unwrap()
        };
        #[cfg(not(windows))]
        let f = std::fs::File::open(p).unwrap();
        f.set_modified(old).unwrap();
    };
    age(&stale);
    age(&stale_other);
    age(&dir_named);
    for f in foreign {
        age(&c3.join(f));
    }
    assert!(is_write_temp_name(
        &stale.file_name().unwrap().to_string_lossy()
    ));
    for f in foreign {
        assert!(!is_write_temp_name(f), "{f}");
    }
    write_in(&r, &c3, "notes.md", |_, _| Ok(b"note\n".to_vec())).unwrap();
    assert!(!stale.exists() && !stale_other.exists());
    assert!(fresh.exists());
    for f in foreign {
        assert!(c3.join(f).exists(), "{f}");
    }
    assert!(dir_named.is_dir());
    assert_eq!(
        std::fs::read_to_string(c3.join("notes.md")).unwrap(),
        "note\n"
    );
    let _ = std::fs::remove_dir_all(&r);
}

// (R7b, F48-4) A collab path is classified by identity too: an alias that is a link to `.eck`
// resolves to the folder (and refuses a writer), and on Windows the 8.3 short name of `.eck`
// (`ECK~1`) does (skipped where the volume makes no short names).
#[test]
fn short_names_and_aliases_resolve_to_the_folder() {
    let r = scratch("identity");
    seed_eck(&r);
    let alias = r.join("alias");
    if dir_link(&alias, &legacy_path(&r)) {
        assert_eq!(
            split_state_path(&r, &alias.join("collab")),
            Some((StateName::Legacy, "collab".to_string()))
        );
        let why = begin_run(&r, &alias.join("collab"), "t", &["handoffs"]).unwrap_err();
        assert!(why.contains("alias is a link"), "{why}");
        drop_link(&alias);
    } else {
        eprintln!("skipped the alias half: the platform made no directory link");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("cmd")
            .raw_arg(format!(
                "/c for %I in (\"{}\") do @echo %~snxI",
                legacy_path(&r).display()
            ))
            .output()
            .unwrap();
        let short = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if short.is_empty() || short.eq_ignore_ascii_case(LEGACY_STATE_DIR) {
            eprintln!("skipped the short-name half: no 8.3 name for .eck ({short:?})");
        } else {
            assert_eq!(
                split_state_path(&r, &r.join(&short).join("collab")),
                Some((StateName::Legacy, "collab".to_string())),
                "{short}"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&r);
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
    let err = write_in(&r, &path(&r).join("scouts"), "x.md", |_, _| {
        Ok(b"x".to_vec())
    })
    .unwrap_err();
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
