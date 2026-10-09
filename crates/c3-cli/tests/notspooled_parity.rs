//! (wave 3b of the 0.6.1 parity) The not-spooled count, its fold and the forgetting marker end to
//! end through the real `c3` binary - the Rust mirror of the plugin's harness-fixes28e NOTSPOOLED
//! (E2, E20, E24, E26) and MARKER (E3) checks and of harness-fixes28d NOTSPOOLED D4 / MARKER D2,
//! on C3's own telemetry root `<codex home>/c3/telemetry/` (P7):
//!
//! - E2: one file per producer process `telemetry-not-spooled-<pid>-<start ticks>.ndjson`;
//!   `--status` sums every producer's file and the legacy files; a flush folds the files of gone
//!   producers (a dead pid, this pid with other start ticks, the legacy file) into ONE note of
//!   `last-flush.json` and keeps the live producer's file; two processes appending at once write two
//!   files without a loss; `--forget --local` removes every one;
//! - E20: the record is saved before the deletes (`CODEX_CONSULT_TEST_FOLD_CRASH=1`: exit 87 in
//!   between; the restart deletes without counting again); a read-only record folds nothing and
//!   warns;
//! - E24: `not_spooled_folded[]` holds `{name, bytes}` (a legacy line appended after the crash
//!   counted once; entries of an earlier build);
//! - E26: the legacy file staged as `telemetry-not-spooled-legacy-<utc ticks>.ndjson`
//!   (`CODEX_CONSULT_TEST_FOLD_CRASH=2`: exit 88 after the deletes; the legacy name recreated with
//!   equal or longer contents); a legacy file a writer holds is skipped with a note;
//! - E3: the forgetting marker judged on pid AND start ticks (the wrong ticks gone - a producer
//!   removes it with a note; the right ticks refuse; an unreadable start alive), the forget's own
//!   marker with `start_ticks`;
//! - wave 3d (the diff-review F24-1..F24-6): a flush without the spool lock folds nothing and
//!   leaves the gone producer's lines unseen (RC1; since wave 3f it sees no line at all); a tail
//!   without a line end appended to a recorded file is counted (RC2); a count made during a local
//!   forget never survives it (RC3, test hook `CODEX_CONSULT_TEST_CLEANUP_GATE`);
//!   `--status` names the latest unseen line; the plugin-home hook's marker and the production
//!   layout without it (RC4); a failed rewrite after the fold's deletes is said and loses no count
//!   (test hook `CODEX_CONSULT_TEST_FOLD_REWRITE_FAIL`);
//! - wave 3f (F31-1): a producer PROCESS live at a flush without the spool lock and gone at the next
//!   fold is counted exactly once (N; N + M with lines appended in between; with the fold's crash
//!   hooks too), and that flush carries the last fold's baseline unchanged.
//!
//! This test process stands in for a LIVE producer (its pid and start ticks); pid 999999 for a gone
//! one. Nothing reaches a real intake (`C3_TELEMETRY_HUB` is a closed loopback port). Windows only
//! (exact start ticks; file sharing modes).
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

use c3::telemetry::notspooled::{self, LocalPaths};
use serde_json::Value;

fn c3_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_c3"))
}

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = std::env::temp_dir().join(format!("c3-ns-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// One not-spooled line as the plugin writes it.
fn line(time: &str, why: &str) -> String {
    format!("{{\"time\":\"{time}\",\"why\":\"{why}\"}}\n")
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// The lines of `out` that start with `prefix`.
fn status_line(out: &str, prefix: &str) -> String {
    out.lines()
        .filter(|l| l.starts_with(prefix))
        .collect::<Vec<_>>()
        .join(" // ")
}

struct H {
    work: PathBuf,
    home: PathBuf,
    dir: PathBuf,
    paths: LocalPaths,
}

fn setup(tag: &str) -> H {
    let work = scratch(tag);
    let home = work.join("home");
    let dir = home.join("c3").join("telemetry");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(work.join("userhome")).unwrap();
    let paths = LocalPaths::in_dir(&dir);
    H {
        work,
        home,
        dir,
        paths,
    }
}

impl H {
    /// `c3` with this test's switches only: test mode on, the intake a closed loopback port.
    fn c3(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        self.cmd(args, env).output().expect("run c3")
    }

    /// The command [`H::c3`] runs (to start it in the background).
    fn cmd(&self, args: &[&str], env: &[(&str, &str)]) -> Command {
        let mut cmd = Command::new(c3_bin());
        for (k, _) in std::env::vars() {
            if k.starts_with("CODEX_CONSULT_") || k.starts_with("FAKE_") || k.starts_with("C3_") {
                cmd.env_remove(&k);
            }
        }
        cmd.current_dir(&self.work)
            .env("CODEX_HOME", &self.home)
            .env("HOME", self.work.join("userhome"))
            .env("USERPROFILE", self.work.join("userhome"))
            .env("CODEX_CONSULT_TEST_MODE", "1")
            .env("CODEX_CONSULT_HEALTH", "none")
            .env("C3_TELEMETRY_HUB", "http://127.0.0.1:9/T")
            .env("C3_PRIORS", "off");
        for (k, v) in env {
            if v.is_empty() {
                cmd.env_remove(k);
            } else {
                cmd.env(k, v);
            }
        }
        cmd.args(args);
        cmd
    }

    fn status(&self) -> String {
        let o = self.c3(&["telemetry", "--status"], &[]);
        assert_eq!(o.status.code(), Some(0), "{}", text(&o));
        text(&o)
    }

    fn flush(&self, env: &[(&str, &str)]) -> Output {
        self.c3(&["telemetry", "--flush", "--telemetry", "on"], env)
    }

    fn last(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(&self.paths.last).unwrap()).unwrap()
    }

    fn last_text(&self) -> String {
        std::fs::read_to_string(&self.paths.last).unwrap_or_default()
    }

    fn notes(&self) -> Vec<String> {
        let Ok(t) = std::fs::read_to_string(&self.paths.last) else {
            return Vec::new();
        };
        let v: Value = serde_json::from_str(&t).unwrap();
        v["notes"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|n| n.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The notes that match `^\S+ <rest>$`.
    fn notes_like(&self, rest: &str) -> usize {
        let re = regex::Regex::new(&format!(r"^\S+ {rest}$")).unwrap();
        self.notes().iter().filter(|n| re.is_match(n)).count()
    }

    fn fold_notes(&self) -> usize {
        let re = regex::Regex::new(r" folded \d+ not-spooled").unwrap();
        self.notes().iter().filter(|n| re.is_match(n)).count()
    }

    /// Every name in the telemetry directory that starts `telemetry-not-spooled`, sorted.
    fn ns_names(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(&self.dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("telemetry-not-spooled"))
            .collect();
        v.sort();
        v
    }

    fn staged(&self) -> Vec<String> {
        self.ns_names()
            .into_iter()
            .filter(|n| n.starts_with("telemetry-not-spooled-legacy-"))
            .collect()
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let p = self.dir.join(name);
        std::fs::write(&p, text).unwrap();
        p
    }

    fn own(&self) -> PathBuf {
        self.paths.own_file()
    }

    fn count(&self) -> notspooled::NsCount {
        notspooled::count(&self.paths, false)
    }

    fn done(self) {
        let _ = std::fs::remove_dir_all(&self.work);
    }
}

fn len(p: &Path) -> u64 {
    std::fs::metadata(p).unwrap().len()
}

fn folded_entries(last: &Value) -> Vec<String> {
    let mut v: Vec<String> = last["not_spooled_folded"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| format!("{}={}", e["name"].as_str().unwrap(), e["bytes"]))
        .collect();
    v.sort();
    v
}

// --------------------------------------------------------------------------- E2

#[test]
fn e2_status_sums_the_producers_and_a_flush_folds_the_gone_ones() {
    let h = setup("e2");
    let me = std::process::id();
    let ticks = notspooled::own_start_ticks();
    assert!(ticks > 0);
    // (fixes28d D4) the count is written WITHOUT any lock: the spool lock held elsewhere does not
    // delay it
    let held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(h.dir.join("spool.lock"))
        .unwrap();
    held.lock().unwrap();
    let t0 = std::time::Instant::now();
    notspooled::append(&h.paths, "own-1").unwrap();
    assert!(t0.elapsed() < Duration::from_secs(1));
    drop(held);
    assert_eq!(
        h.own().file_name().unwrap().to_string_lossy(),
        format!("telemetry-not-spooled-{me}-{ticks}.ndjson")
    );
    h.write(
        "telemetry-not-spooled-999999-639000000000000000.ndjson",
        &(line("2026-10-01T10:00:00+02:00", "gone-1")
            + &line("2026-10-01T10:01:00+02:00", "gone-2")),
    );
    h.write(
        &format!("telemetry-not-spooled-{me}-{}.ndjson", ticks - 10_000_000),
        &(line("2026-10-02T10:00:00+02:00", "reused-1")
            + "{\"time\":\"2026-10-02T10:01:00+02:00\",\"why\":\"half"),
    );
    h.write(
        "telemetry-not-spooled.ndjson",
        &(line("2026-09-01T10:00:00+02:00", "legacy-1")
            + &line("2026-09-01T10:01:00+02:00", "legacy-2")
            + &line("2026-09-01T10:02:00+02:00", "legacy-3")),
    );
    h.write(
        "telemetry-not-spooled-notes.ndjson",
        &line("2026-10-03T10:00:00+02:00", "not a count"),
    );
    let st = h.status();
    let re = regex::Regex::new(r"(?m)^not spooled: 7 event\(s\) since the last flush - the latest \S+: own-1 \(7 line\(s\) in 4 file\(s\), one per producer").unwrap();
    assert!(re.is_match(&st), "{}", status_line(&st, "not spooled"));
    let fl = h.flush(&[]);
    assert_eq!(fl.status.code(), Some(0), "{}", text(&fl));
    let last = h.last();
    assert_eq!(last["not_spooled_seen"], 1);
    assert_eq!(
        h.notes_like(r"folded 7 not-spooled line\(s\) of 3 gone producer\(s\)"),
        1,
        "{:?}",
        h.notes()
    );
    assert_eq!(h.fold_notes(), 1);
    let own_name = h.own().file_name().unwrap().to_string_lossy().to_string();
    let mut want = vec![
        own_name.clone(),
        "telemetry-not-spooled-notes.ndjson".to_string(),
    ];
    want.sort();
    assert_eq!(h.ns_names(), want);
    // the record's field list (harness-telemetry SEND: the plugin's .last)
    let keys: Vec<&str> = last
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys.join(","),
        "time,result,delivered,kept,dropped,rejected,http,not_spooled_seen,not_spooled_folded,notes"
    );
    let st2 = h.status();
    assert!(
        st2.contains("\nnot spooled: none since the last flush (1 line(s) in 1 file(s)"),
        "{}",
        status_line(&st2, "not spooled")
    );
    let re = regex::Regex::new(
        r"(?m)^note       : \S+ folded 7 not-spooled line\(s\) of 3 gone producer\(s\)$",
    )
    .unwrap();
    assert!(re.is_match(&st2), "{}", status_line(&st2, "note"));
    // a line after the flush counts 1; the next flush folds nothing and keeps the file
    notspooled::append(&h.paths, "own-2").unwrap();
    let n3 = h.count();
    assert_eq!((n3.count, n3.last.as_str()), (1, "own-2"));
    let fl2 = h.flush(&[]);
    assert_eq!(fl2.status.code(), Some(0), "{}", text(&fl2));
    assert_eq!(h.last()["not_spooled_seen"], 2);
    assert_eq!(h.fold_notes(), 1);
    assert_eq!(std::fs::read_to_string(h.own()).unwrap().lines().count(), 2);
    let n4 = h.count();
    assert_eq!((n4.count, n4.total), (0, 2));
    // C3's own wave-2 single file is a legacy file too: counted whole, staged and folded
    h.write(
        "not-spooled.ndjson",
        &(line("2026-09-01T11:00:00+02:00", "c3-old-1")
            + &line("2026-09-01T11:01:00+02:00", "c3-old-2")),
    );
    assert_eq!(h.count().count, 2);
    let fl3 = h.flush(&[]);
    assert_eq!(fl3.status.code(), Some(0), "{}", text(&fl3));
    assert_eq!(
        h.notes_like(r"folded 2 not-spooled line\(s\) of 1 gone producer\(s\)"),
        1,
        "{:?}",
        h.notes()
    );
    assert!(!h.dir.join("not-spooled.ndjson").exists());
    assert!(h.staged().is_empty());
    assert_eq!(h.count().count, 0);
    h.done();
}

/// The child half of the two-producer race: appends `C3_NS_CHILD_N` lines to its OWN file once the
/// barrier opens, then names its file. Without `C3_NS_CHILD_DIR` it does nothing.
#[test]
fn ns_child_appender() {
    let Ok(dir) = std::env::var("C3_NS_CHILD_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let id = std::env::var("C3_NS_CHILD_ID").unwrap();
    let n: usize = std::env::var("C3_NS_CHILD_N").unwrap().parse().unwrap();
    let home = PathBuf::from(std::env::var("C3_NS_CHILD_HOME").unwrap());
    let paths = LocalPaths::in_dir(&home);
    std::fs::write(dir.join(format!("ready-{id}")), "r").unwrap();
    let t0 = std::time::Instant::now();
    while !dir.join("go").exists() && t0.elapsed() < Duration::from_secs(60) {
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut bad = 0;
    for i in 1..=n {
        if notspooled::append(&paths, &format!("child {id} line {i}")).is_err() {
            bad += 1;
        }
    }
    std::fs::write(
        dir.join(format!("result-{id}")),
        format!(
            "{}|{bad}|{}",
            paths.own_file().file_name().unwrap().to_string_lossy(),
            std::process::id()
        ),
    )
    .unwrap();
}

#[test]
fn e2_two_producers_appending_at_once_write_two_files_and_lose_nothing() {
    let h = setup("race");
    let race = h.work.join("race");
    std::fs::create_dir_all(&race).unwrap();
    let exe = std::env::current_exe().unwrap();
    let mut kids = Vec::new();
    for id in 1..=2 {
        let mut cmd = Command::new(&exe);
        for (k, _) in std::env::vars() {
            if k.starts_with("CODEX_CONSULT_") || k.starts_with("C3_") {
                cmd.env_remove(&k);
            }
        }
        kids.push(
            cmd.args([
                "--exact",
                "ns_child_appender",
                "--nocapture",
                "--test-threads",
                "1",
            ])
            .env("C3_NS_CHILD_DIR", &race)
            .env("C3_NS_CHILD_ID", id.to_string())
            .env("C3_NS_CHILD_N", "40")
            .env("C3_NS_CHILD_HOME", &h.dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
        );
    }
    let t0 = std::time::Instant::now();
    while (0..2).any(|i| !race.join(format!("ready-{}", i + 1)).exists())
        && t0.elapsed() < Duration::from_secs(60)
    {
        std::thread::sleep(Duration::from_millis(50));
    }
    std::fs::write(race.join("go"), "go").unwrap();
    for mut k in kids {
        assert!(k.wait().unwrap().success());
    }
    let res: Vec<Vec<String>> = (1..=2)
        .map(|i| {
            std::fs::read_to_string(race.join(format!("result-{i}")))
                .unwrap()
                .split('|')
                .map(str::to_string)
                .collect()
        })
        .collect();
    assert_ne!(res[0][0], res[1][0]);
    for r in &res {
        assert_eq!(r[1], "0", "an append failed: {r:?}");
        assert!(
            r[0].starts_with(&format!("telemetry-not-spooled-{}-", r[2])),
            "{r:?}"
        );
        let lines = std::fs::read_to_string(h.dir.join(&r[0]))
            .unwrap()
            .lines()
            .count();
        assert_eq!(lines, 40);
    }
    let st = h.status();
    assert!(
        st.contains("\nnot spooled: 80 event(s) since the last flush - the latest "),
        "{}",
        status_line(&st, "not spooled")
    );
    // both producers have exited: the next flush folds their files
    let fl = h.flush(&[]);
    assert_eq!(fl.status.code(), Some(0), "{}", text(&fl));
    assert_eq!(
        h.notes_like(r"folded 80 not-spooled line\(s\) of 2 gone producer\(s\)"),
        1,
        "{:?}",
        h.notes()
    );
    assert!(h.ns_names().is_empty(), "{:?}", h.ns_names());
    assert_eq!(h.count().count, 0);
    h.done();
}

#[test]
fn e2_forget_local_removes_every_not_spooled_file_and_leaves_a_foreign_name() {
    let h = setup("forget");
    notspooled::append(&h.paths, "own").unwrap();
    let gone = h.write(
        "telemetry-not-spooled-999999-639000000000000000.ndjson",
        &line("2026-10-01T10:00:00+02:00", "gone-again"),
    );
    h.write(
        "telemetry-not-spooled.ndjson",
        &line("2026-09-01T10:00:00+02:00", "legacy-again"),
    );
    h.write(
        "telemetry-not-spooled-legacy-639000000000000001.ndjson",
        &line("2026-09-01T10:00:00+02:00", "staged"),
    );
    h.write(
        "telemetry-not-spooled-notes.ndjson",
        &line("2026-10-03T10:00:00+02:00", "not a count"),
    );
    let fg = h.c3(&["telemetry", "--forget", "--local", "--yes"], &[]);
    let out = text(&fg);
    assert_eq!(fg.status.code(), Some(0), "{out}");
    assert!(out.contains("removed locally - "), "{out}");
    for n in [
        h.own().file_name().unwrap().to_string_lossy().to_string(),
        "telemetry-not-spooled.ndjson".to_string(),
        gone.file_name().unwrap().to_string_lossy().to_string(),
        "telemetry-not-spooled-legacy-639000000000000001.ndjson".to_string(),
    ] {
        assert!(out.contains(&n), "{n} not named: {out}");
    }
    assert_eq!(h.ns_names(), vec!["telemetry-not-spooled-notes.ndjson"]);
    assert!(
        !h.paths.marker.exists(),
        "the marker never outlives the forget"
    );
    h.done();
}

// --------------------------------------------------------------------------- E20

#[test]
fn e20_the_record_is_saved_before_the_deletes_and_a_crash_in_between_is_not_counted_twice() {
    let h = setup("e20");
    notspooled::append(&h.paths, "live-1").unwrap();
    notspooled::append(&h.paths, "live-2").unwrap();
    let fa = h.flush(&[]);
    assert_eq!(fa.status.code(), Some(0), "{}", text(&fa));
    assert_eq!(h.last()["not_spooled_seen"], 2);
    let gone = h.write(
        "telemetry-not-spooled-999999-639000000000000001.ndjson",
        &(line("2026-10-07T10:00:00+02:00", "gone-a1")
            + &line("2026-10-07T10:01:00+02:00", "gone-a2")
            + &line("2026-10-07T10:02:00+02:00", "gone-a3")),
    );
    let legacy = h.write(
        "telemetry-not-spooled.ndjson",
        &(line("2026-09-01T10:00:00+02:00", "legacy-a1")
            + &line("2026-09-01T10:01:00+02:00", "legacy-a2")),
    );
    let len_legacy = len(&legacy);
    assert_eq!(h.count().count, 5);
    let crash = h.flush(&[("CODEX_CONSULT_TEST_FOLD_CRASH", "1")]);
    assert_eq!(crash.status.code(), Some(87), "{}", text(&crash));
    let staged = h.staged();
    assert_eq!(
        staged.len(),
        1,
        "(E26) the legacy file staged under a unique name"
    );
    assert!(!legacy.exists(), "the legacy name is free");
    assert!(
        gone.exists(),
        "nothing deleted before the save... and the crash came after it"
    );
    let last = h.last();
    assert_eq!(last["not_spooled_seen"], 2);
    let mut want = vec![
        format!(
            "{}={}",
            gone.file_name().unwrap().to_string_lossy(),
            len(&gone)
        ),
        format!("{}={len_legacy}", staged[0]),
    ];
    want.sort();
    assert_eq!(folded_entries(&last), want);
    assert_eq!(h.fold_notes(), 1);
    assert_eq!(
        h.notes_like(r"folded 5 not-spooled line\(s\) of 2 gone producer\(s\)"),
        1
    );
    let fold_note = h
        .notes()
        .into_iter()
        .find(|n| n.contains(" folded "))
        .unwrap();
    // meanwhile --status leaves out the files the record names
    let nc = h.count();
    assert_eq!((nc.count, nc.total, nc.files), (0, 2, 1));
    let st = h.status();
    assert!(
        st.contains("\nnot spooled: none since the last flush (2 line(s) in 1 file(s)"),
        "{}",
        status_line(&st, "not spooled")
    );
    // the restarted flush deletes them WITHOUT counting them again
    let restart = h.flush(&[]);
    assert_eq!(restart.status.code(), Some(0), "{}", text(&restart));
    assert!(!gone.exists());
    assert!(h.staged().is_empty());
    let last = h.last();
    assert_eq!(last["not_spooled_seen"], 2);
    assert_eq!(last["not_spooled_folded"], serde_json::json!([]));
    assert_eq!(h.fold_notes(), 1);
    assert!(h.notes().contains(&fold_note), "the same note, kept once");
    notspooled::append(&h.paths, "live-3").unwrap();
    let st = h.status();
    let re = regex::Regex::new(r"(?m)^not spooled: 1 event\(s\) since the last flush - the latest \S+: live-3 \(3 line\(s\) in 1 file\(s\)").unwrap();
    assert!(re.is_match(&st), "{}", status_line(&st, "not spooled"));

    // a record that cannot be written (read-only): the flush folds NOTHING and warns
    let gone_b = h.write(
        "telemetry-not-spooled-999999-639000000000000002.ndjson",
        &(line("2026-10-07T11:00:00+02:00", "gone-b1")
            + &line("2026-10-07T11:01:00+02:00", "gone-b2")),
    );
    let before = h.last_text();
    assert_eq!(h.count().count, 3);
    let mut perm = std::fs::metadata(&h.paths.last).unwrap().permissions();
    perm.set_readonly(true);
    std::fs::set_permissions(&h.paths.last, perm.clone()).unwrap();
    let fw = h.flush(&[]);
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    std::fs::set_permissions(&h.paths.last, perm).unwrap();
    let out = text(&fw);
    assert_eq!(fw.status.code(), Some(0), "{out}");
    assert!(
        out.contains(&format!(
            "; warning: {} could not be written (",
            h.paths.last.display()
        )) && out.contains(") - nothing was folded: the not-spooled files and the last baseline stay, the next flush counts them"),
        "{out}"
    );
    assert!(gone_b.exists());
    assert_eq!(h.last_text(), before, "the record byte-identical");
    let nw = h.count();
    assert_eq!((nw.count, nw.total), (3, 5));
    // writable again: the next flush folds it
    let ok = h.flush(&[]);
    assert_eq!(ok.status.code(), Some(0), "{}", text(&ok));
    assert!(!text(&ok).contains("warning"), "{}", text(&ok));
    assert!(!gone_b.exists());
    assert_eq!(
        h.notes_like(r"folded 2 not-spooled line\(s\) of 1 gone producer\(s\)"),
        1
    );
    assert_eq!(h.last()["not_spooled_seen"], 3);
    assert_eq!(h.count().count, 0);
    h.done();
}

// --------------------------------------------------------------------------- E24

#[test]
fn e24_a_legacy_line_appended_after_the_crash_is_counted_exactly_once() {
    let h = setup("e24");
    notspooled::append(&h.paths, "live-g1").unwrap();
    assert_eq!(h.flush(&[]).status.code(), Some(0));
    let legacy = h.write(
        "telemetry-not-spooled.ndjson",
        &(line("2026-09-02T10:00:00+02:00", "legacy-g1")
            + &line("2026-09-02T10:01:00+02:00", "legacy-g2")),
    );
    let len_g = len(&legacy);
    let crash = h.flush(&[("CODEX_CONSULT_TEST_FOLD_CRASH", "1")]);
    assert_eq!(crash.status.code(), Some(87), "{}", text(&crash));
    let ent = h.last()["not_spooled_folded"].as_array().unwrap().clone();
    assert_eq!(ent.len(), 1);
    let re = regex::Regex::new(r"^telemetry-not-spooled-legacy-\d+\.ndjson$").unwrap();
    assert!(re.is_match(ent[0]["name"].as_str().unwrap()), "{ent:?}");
    assert_eq!(ent[0]["bytes"], len_g);
    // an older bridge appends one more line to the legacy name
    let t = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&legacy)
            .unwrap();
        f.write_all(line(&t, "legacy-g3 after the crash").as_bytes())
            .unwrap();
    }
    let ng = h.count();
    assert_eq!(
        (ng.count, ng.last.as_str()),
        (1, "legacy-g3 after the crash")
    );
    let restart = h.flush(&[]);
    assert_eq!(restart.status.code(), Some(0), "{}", text(&restart));
    assert!(!legacy.exists());
    assert_eq!(
        h.notes_like(r"folded 2 not-spooled line\(s\) of 1 gone producer\(s\)"),
        1
    );
    assert_eq!(
        h.notes_like(r"folded 1 not-spooled line\(s\) of 1 gone producer\(s\)"),
        1
    );
    assert_eq!(h.fold_notes(), 2);
    let last = h.last();
    assert_eq!(last["not_spooled_folded"], serde_json::json!([]));
    assert_eq!(last["not_spooled_seen"], 1);
    assert_eq!(h.count().count, 0);

    // entries of an earlier build: one naming the LEGACY file is ignored (shorter than recorded or
    // of the recorded length alike); a bare name of a gone producer's file counts nothing
    let seed = |folded: Value| {
        let v = serde_json::json!({"time": "2026-10-07T10:00:00+02:00", "result": "seeded", "delivered": 0, "kept": 0, "dropped": 0, "rejected": [], "http": null, "not_spooled_seen": 1, "not_spooled_folded": folded, "notes": []});
        std::fs::write(&h.paths.last, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    };
    h.write(
        "telemetry-not-spooled.ndjson",
        &line("2026-09-03T10:00:00+02:00", "legacy-new"),
    );
    seed(serde_json::json!([{"name": "telemetry-not-spooled.ndjson", "bytes": 99999}]));
    assert_eq!(h.count().count, 1);
    assert_eq!(h.flush(&[]).status.code(), Some(0));
    assert_eq!(
        h.notes_like(r"folded 1 not-spooled line\(s\) of 1 gone producer\(s\)"),
        1
    );
    let same = h.write(
        "telemetry-not-spooled.ndjson",
        &line("2026-09-03T11:00:00+02:00", "legacy-same"),
    );
    seed(serde_json::json!([{"name": "telemetry-not-spooled.ndjson", "bytes": len(&same)}]));
    assert_eq!(h.count().count, 1);
    assert_eq!(h.flush(&[]).status.code(), Some(0));
    assert_eq!(
        h.notes_like(r"folded 1 not-spooled line\(s\) of 1 gone producer\(s\)"),
        1
    );
    let bare = h.write(
        "telemetry-not-spooled-999999-639000000000000009.ndjson",
        &line("2026-09-04T10:00:00+02:00", "bare"),
    );
    seed(serde_json::json!([
        "telemetry-not-spooled-999999-639000000000000009.ndjson"
    ]));
    assert_eq!(h.count().count, 0);
    assert_eq!(h.flush(&[]).status.code(), Some(0));
    assert_eq!(h.fold_notes(), 0, "{:?}", h.notes());
    assert!(!bare.exists());
    assert!(!h.dir.join("telemetry-not-spooled.ndjson").exists());

    // a SHORTER file under a recorded producer name is another file: folded afresh
    let other = h.write(
        "telemetry-not-spooled-999999-639000000000000010.ndjson",
        &line("2026-09-05T10:00:00+02:00", "short"),
    );
    seed(
        serde_json::json!([{"name": "telemetry-not-spooled-999999-639000000000000010.ndjson", "bytes": 99999}]),
    );
    // counted whole (the recorded bytes are longer than the file): this process's line and its
    // line, less the seeded baseline 1
    let ns = h.count();
    assert_eq!((ns.count, ns.total, ns.last.as_str()), (1, 2, "live-g1"));
    assert_eq!(h.flush(&[]).status.code(), Some(0));
    assert_eq!(
        h.notes_like(r"folded 1 not-spooled line\(s\) of 1 gone producer\(s\)"),
        1
    );
    assert!(!other.exists());
    h.done();
}

// --------------------------------------------------------------------------- E26

#[test]
fn e26_a_crash_after_the_deletes_and_the_legacy_name_recreated_counts_every_line_once() {
    for (tag, n) in [("a", 2usize), ("b", 3usize)] {
        let h = setup(&format!("e26{tag}"));
        notspooled::append(&h.paths, &format!("live-{tag}")).unwrap();
        assert_eq!(h.flush(&[]).status.code(), Some(0));
        let leg = |i: usize, t: &str| {
            line(
                &format!("2026-09-05T10:0{i}:00+02:00"),
                &format!("legacy-{t}{i}"),
            )
        };
        let legacy = h.write(
            "telemetry-not-spooled.ndjson",
            &(leg(1, "o") + &leg(2, "o")),
        );
        let len_k = len(&legacy);
        let crash = h.flush(&[("CODEX_CONSULT_TEST_FOLD_CRASH", "2")]);
        assert_eq!(crash.status.code(), Some(88), "{}", text(&crash));
        let ent = h.last()["not_spooled_folded"].as_array().unwrap().clone();
        assert_eq!(ent.len(), 1, "{ent:?}");
        assert!(ent[0]["name"]
            .as_str()
            .unwrap()
            .starts_with("telemetry-not-spooled-legacy-"));
        assert_eq!(ent[0]["bytes"], len_k);
        assert!(h.staged().is_empty(), "staged, counted and deleted");
        assert!(!legacy.exists(), "the legacy name is free");
        assert_eq!(
            h.notes_like(r"folded 2 not-spooled line\(s\) of 1 gone producer\(s\)"),
            1
        );
        // an older bridge recreates the legacy name
        let body: String = (1..=n).map(|i| leg(i, "n")).collect();
        h.write("telemetry-not-spooled.ndjson", &body);
        let len_new = len(&legacy);
        if n == 2 {
            assert_eq!(len_new, len_k);
        } else {
            assert!(len_new > len_k);
        }
        assert_eq!(h.count().count, n as u64);
        let restart = h.flush(&[]);
        assert_eq!(restart.status.code(), Some(0), "{}", text(&restart));
        let want_two = if n == 2 { 2 } else { 1 };
        assert_eq!(
            h.notes_like(r"folded 2 not-spooled line\(s\) of 1 gone producer\(s\)"),
            want_two,
            "{:?}",
            h.notes()
        );
        assert_eq!(
            h.notes_like(&format!(
                r"folded {n} not-spooled line\(s\) of 1 gone producer\(s\)"
            )),
            want_two
        );
        assert_eq!(h.fold_notes(), 2);
        assert_eq!(
            h.ns_names(),
            vec![h.own().file_name().unwrap().to_string_lossy().to_string()]
        );
        assert_eq!(h.last()["not_spooled_folded"], serde_json::json!([]));
        assert_eq!(h.count().count, 0);
        h.done();
    }
}

#[test]
fn e26_a_legacy_file_a_writer_holds_is_skipped_with_a_note_and_folded_once_free() {
    let h = setup("e26busy");
    let legacy = h.write(
        "telemetry-not-spooled.ndjson",
        &(line("2026-09-05T10:01:00+02:00", "legacy-h1")
            + &line("2026-09-05T10:02:00+02:00", "legacy-h2")),
    );
    let busy = {
        use std::os::windows::fs::OpenOptionsExt;
        let hold = std::fs::OpenOptions::new()
            .append(true)
            .share_mode(0x1) // FILE_SHARE_READ: a writer that lets readers in, not a rename
            .open(&legacy)
            .unwrap();
        let o = h.flush(&[]);
        drop(hold);
        o
    };
    assert_eq!(busy.status.code(), Some(0), "{}", text(&busy));
    let re = format!(
        r"the legacy not-spooled file {} could not be staged \(.+\) - not folded this flush",
        regex::escape(&legacy.display().to_string())
    );
    assert_eq!(h.notes_like(&re), 1, "{:?}", h.notes());
    assert_eq!(h.fold_notes(), 0);
    assert_eq!(h.last()["not_spooled_seen"], 0, "its lines are never seen");
    assert_eq!(h.count().count, 2);
    let free = h.flush(&[]);
    assert_eq!(free.status.code(), Some(0), "{}", text(&free));
    assert_eq!(
        h.notes_like(r"folded 2 not-spooled line\(s\) of 1 gone producer\(s\)"),
        1
    );
    assert!(!legacy.exists());
    assert_eq!(h.count().count, 0);
    assert!(h.ns_names().is_empty());
    h.done();
}

// --------------------------------------------------------------------------- E3

#[test]
fn e3_the_marker_is_judged_on_pid_and_start_ticks() {
    let h = setup("marker");
    let me = std::process::id();
    let ticks = notspooled::own_start_ticks();
    let mk = &h.paths.marker;
    // this pid with another start in ticks: GONE - --status says so, a producer removes it (a
    // note) and spools its event
    std::fs::write(
        mk,
        format!(
            "{{\"pid\":{me},\"start_ticks\":{},\"since\":\"2026-10-07T01:00:00+02:00\"}}\n",
            ticks + 1
        ),
    )
    .unwrap();
    let st = h.status();
    let re = regex::Regex::new(&format!(
        r"(?m)^forgetting : the marker .* - its owner pid {me} is gone"
    ))
    .unwrap();
    assert!(re.is_match(&st), "{}", status_line(&st, "forgetting"));
    let spool = c3::telemetry::Spool::with_paths(&h.dir, "http://127.0.0.1:9/T", h.paths.clone());
    spool
        .enqueue_line_within(&serde_json::json!({"probe": 1}), Duration::from_secs(1))
        .unwrap();
    assert!(!mk.exists());
    assert_eq!(spool.pending(), 1);
    assert_eq!(
        h.notes_like(&format!(
            r"removed the forgetting marker of pid {me} \(gone\) since 2026-10-07T01:00:00\+02:00 - .*"
        )),
        1,
        "{:?}",
        h.notes()
    );
    // the right pid AND ticks: the owner LIVES - a producer is refused at once, the marker stays,
    // the sender sends nothing
    std::fs::write(
        mk,
        format!(
            "{{\"pid\":{me},\"start_ticks\":{ticks},\"since\":\"2026-10-07T02:00:00+02:00\"}}\n"
        ),
    )
    .unwrap();
    let t0 = std::time::Instant::now();
    let refused = spool
        .enqueue_line_within(&serde_json::json!({"probe": 2}), Duration::from_secs(5))
        .unwrap_err()
        .to_string();
    assert!(t0.elapsed() < Duration::from_secs(2), "refused at once");
    assert!(
        refused.starts_with(&format!(
            "c3 telemetry --forget --local is deleting the local telemetry data (pid {me}, since 2026-10-07T02:00:00+02:00; the marker "
        )),
        "{refused}"
    );
    assert!(mk.exists());
    assert_eq!(spool.pending(), 1);
    let st = h.status();
    let re = regex::Regex::new(&format!(
        r"(?m)^forgetting : the marker .* - its owner pid {me} lives"
    ))
    .unwrap();
    assert!(re.is_match(&st), "{}", status_line(&st, "forgetting"));
    let fl = h.flush(&[]);
    assert_eq!(fl.status.code(), Some(1), "{}", text(&fl));
    assert!(
        text(&fl).contains("skipped - c3 telemetry --forget --local is deleting"),
        "{}",
        text(&fl)
    );
    assert_eq!(spool.pending(), 1, "nothing sent");
    // the owner's start unreadable now (the test hook): identity unknown - alive
    std::fs::write(
        mk,
        format!(
            "{{\"pid\":{me},\"start_ticks\":{},\"since\":\"x\"}}\n",
            ticks + 1
        ),
    )
    .unwrap();
    let su = h.c3(
        &["telemetry", "--status"],
        &[("CODEX_CONSULT_TEST_START_UNREADABLE", &me.to_string())],
    );
    assert!(
        text(&su).contains(&format!("its owner pid {me} lives")),
        "{}",
        status_line(&text(&su), "forgetting")
    );
    // an older marker without start_ticks: judged by its start_time; a pid that does not run: gone
    let iso = c3::liveness::proc::process_start_iso(me).unwrap();
    std::fs::write(
        mk,
        format!("{{\"pid\":{me},\"start_time\":\"{iso}\",\"since\":\"x\"}}\n"),
    )
    .unwrap();
    assert!(h.status().contains(&format!("its owner pid {me} lives")));
    std::fs::write(
        mk,
        format!(
            "{{\"pid\":{me},\"start_time\":\"2000-01-01T00:00:00.0000000Z\",\"since\":\"x\"}}\n"
        ),
    )
    .unwrap();
    assert!(h.status().contains(&format!("its owner pid {me} is gone")));
    std::fs::write(
        mk,
        format!("{{\"pid\":999999,\"start_ticks\":{ticks},\"since\":\"x\"}}\n"),
    )
    .unwrap();
    assert!(h.status().contains("its owner pid 999999 is gone"));
    // the sender removes a gone owner's marker and goes on (a closed port: not delivered)
    let fl = h.flush(&[]);
    assert!(!mk.exists(), "{}", text(&fl));
    assert!(!text(&fl).contains("is deleting"), "{}", text(&fl));
    assert_eq!(
        h.notes_like(r"removed the forgetting marker of pid 999999 \(gone\) since x - .*"),
        1,
        "{:?}",
        h.notes()
    );
    h.done();
}

#[test]
fn e3_a_local_forget_holds_a_marker_with_its_start_ticks_and_removes_it_last() {
    let h = setup("forget-marker");
    notspooled::append(&h.paths, "own").unwrap();
    let seen = std::cell::RefCell::new(None::<Value>);
    let remove = |p: &Path| {
        if seen.borrow().is_none() {
            let t = std::fs::read_to_string(&h.paths.marker).unwrap_or_default();
            *seen.borrow_mut() = serde_json::from_str(&t).ok();
        }
        std::fs::remove_file(p)
    };
    let hub = c3::telemetry::Hub {
        base: "http://127.0.0.1:9/T".into(),
        source: "test".into(),
        error: String::new(),
    };
    let out = c3::telemetry::forget_with(
        &hub,
        &h.dir,
        &c3::telemetry::ForgetRequest {
            public_ref: None,
            local: true,
            yes: true,
        },
        |_| true,
        &remove,
    );
    assert_eq!(out.exit, 0, "{:?}", out.lines);
    let m = seen
        .borrow()
        .clone()
        .expect("the marker was there during the cleanup");
    assert_eq!(m["pid"], std::process::id());
    assert_eq!(m["start_ticks"], notspooled::own_start_ticks());
    assert!(m["start_time"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(m["since"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(!h.paths.marker.exists(), "removed last");
    assert!(h.ns_names().is_empty());
    h.done();
}

// --------------------------------------------------------------------------- the test hook

#[test]
fn the_plugin_home_hook_is_honoured_in_test_mode_only() {
    let h = setup("hook");
    std::fs::write(
        h.home.join("telemetry-not-spooled.ndjson"),
        line("2026-09-01T10:00:00+02:00", "plugin-place"),
    )
    .unwrap();
    let home = h.home.to_string_lossy().to_string();
    let on = h.c3(
        &["telemetry", "--status"],
        &[(notspooled::PLUGIN_HOME_VAR, &home)],
    );
    assert!(
        text(&on).contains("\nnot spooled: 1 event(s) since the last flush - the latest 2026-09-01T10:00:00+02:00: plugin-place"),
        "{}",
        text(&on)
    );
    let off = h.c3(
        &["telemetry", "--status"],
        &[
            (notspooled::PLUGIN_HOME_VAR, &home),
            ("CODEX_CONSULT_TEST_MODE", ""),
        ],
    );
    assert!(
        text(&off).contains("\nnot spooled: none since the last flush\n"),
        "{}",
        text(&off)
    );
    // the fold writes the plugin's .last under the hook
    let fl = h.c3(
        &["telemetry", "--flush", "--telemetry", "on"],
        &[(notspooled::PLUGIN_HOME_VAR, &home)],
    );
    assert_eq!(fl.status.code(), Some(0), "{}", text(&fl));
    let last: Value = serde_json::from_str(
        &std::fs::read_to_string(h.home.join("telemetry-spool").join(".last")).unwrap(),
    )
    .unwrap();
    assert_eq!(last["not_spooled_seen"], 0);
    assert!(!h.home.join("telemetry-not-spooled.ndjson").exists());
    h.done();
}

// --------------------------------------------------------------------------- wave 3d (F24-1..F24-6)

/// Start `c3 telemetry --forget --local --yes` with the cleanup gate (test hook
/// `CODEX_CONSULT_TEST_CLEANUP_GATE`) and wait until its cleanup has listed the files and waits.
fn forget_at_the_gate(h: &H, env: &[(&str, &str)]) -> (std::process::Child, PathBuf) {
    let gate = h.work.join(format!(
        "gate-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let gate_text = gate.to_string_lossy().to_string();
    let mut all: Vec<(&str, &str)> = env.to_vec();
    all.push(("CODEX_CONSULT_TEST_CLEANUP_GATE", &gate_text));
    let child = h
        .cmd(&["telemetry", "--forget", "--local", "--yes"], &all)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let waiting = PathBuf::from(format!("{gate_text}.waiting"));
    let t0 = std::time::Instant::now();
    while !waiting.exists() && t0.elapsed() < Duration::from_secs(60) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(waiting.exists(), "the cleanup never reached its gate");
    (child, gate)
}

/// Open the gate and wait for the forget: its exit code and output.
fn open_the_gate(child: std::process::Child, gate: &Path) -> (Option<i32>, String) {
    std::fs::write(gate, "go").unwrap();
    let o = child.wait_with_output().unwrap();
    (o.status.code(), text(&o))
}

/// (F24-1, RC1; wave 3f, F31-1) A flush that cannot take the spool lock folds nothing and sees no
/// line (the baseline stays the last fold's - none here: 0): the gone producer's 3 lines and this
/// live producer's 1 stay unseen - `--status` counts all 4 -, and the next flush folds the gone
/// ones ONCE and sees the live one. One accounting of N.
#[test]
fn f24_1_a_flush_without_the_spool_lock_sees_nothing_and_the_next_folds_the_gone_once() {
    let h = setup("f24-1");
    notspooled::append(&h.paths, "own-1").unwrap();
    let gone = h.write(
        "telemetry-not-spooled-999999-639000000000000000.ndjson",
        &(line("2026-10-01T10:00:00+02:00", "gone-1")
            + &line("2026-10-01T10:01:00+02:00", "gone-2")
            + &line("2026-10-01T10:02:00+02:00", "gone-3")),
    );
    assert_eq!(h.count().count, 4);
    let held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(h.dir.join("spool.lock"))
        .unwrap();
    held.lock().unwrap();
    let fl1 = h.flush(&[]);
    let last1 = h.last();
    drop(held);
    assert_eq!(
        last1["not_spooled_seen"],
        0,
        "{} | {}",
        text(&fl1),
        h.last_text()
    );
    assert!(folded_entries(&last1).is_empty(), "{}", h.last_text());
    assert_eq!(h.fold_notes(), 0, "{:?}", h.notes());
    assert!(gone.exists());
    let st = h.status();
    assert!(
        st.contains("\nnot spooled: 4 event(s) since the last flush - the latest "),
        "{}",
        status_line(&st, "not spooled")
    );
    let fl2 = h.flush(&[]);
    assert_eq!(fl2.status.code(), Some(0), "{}", text(&fl2));
    assert_eq!(
        h.notes_like(r"folded 3 not-spooled line\(s\) of 1 gone producer\(s\)"),
        1,
        "{:?}",
        h.notes()
    );
    assert_eq!(h.fold_notes(), 1);
    assert_eq!(h.last()["not_spooled_seen"], 1);
    assert!(!gone.exists());
    assert_eq!(h.count().count, 0);
    h.done();
}

// --------------------------------------------------------------------------- wave 3f (F31-1)

/// (wave 3f) A PRODUCER PROCESS for the F31-1 fixtures: it appends its batches (`C3_NS_F31_BATCHES`,
/// e.g. `3,2`) to its own not-spooled file under `C3_NS_F31_HOME` - the first at once, each next
/// one once `go-<i>` appears in `C3_NS_F31_DIR` -, writes `ready-<i>` (its file's name) after each,
/// and exits once `go-<batches>` appears. Without `C3_NS_F31_DIR` it does nothing.
#[test]
fn f31_child_producer() {
    let Ok(dir) = std::env::var("C3_NS_F31_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let paths = LocalPaths::in_dir(Path::new(&std::env::var("C3_NS_F31_HOME").unwrap()));
    let batches: Vec<usize> = std::env::var("C3_NS_F31_BATCHES")
        .unwrap()
        .split(',')
        .map(|n| n.trim().parse().unwrap())
        .collect();
    let name = paths
        .own_file()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    for i in 0..=batches.len() {
        if i > 0 {
            let t0 = std::time::Instant::now();
            while !dir.join(format!("go-{i}")).exists() && t0.elapsed() < Duration::from_secs(120) {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let Some(n) = batches.get(i) else {
            break;
        };
        for k in 1..=*n {
            notspooled::append(&paths, &format!("batch {i} line {k}")).unwrap();
        }
        let tmp = dir.join(format!("ready-{i}.tmp"));
        std::fs::write(&tmp, &name).unwrap();
        std::fs::rename(&tmp, dir.join(format!("ready-{i}"))).unwrap();
    }
}

/// (wave 3f) A LIVE producer process ([`f31_child_producer`]) the test steps through its batches.
struct Producer {
    child: std::process::Child,
    sync: PathBuf,
    file: PathBuf,
    step: usize,
}

impl Producer {
    /// Start it: its first batch is in its file when this returns.
    fn start(h: &H, batches: &str) -> Producer {
        let sync = h.work.join("producer");
        std::fs::create_dir_all(&sync).unwrap();
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        for (k, _) in std::env::vars() {
            if k.starts_with("CODEX_CONSULT_") || k.starts_with("C3_") {
                cmd.env_remove(&k);
            }
        }
        let child = cmd
            .args([
                "--exact",
                "f31_child_producer",
                "--nocapture",
                "--test-threads",
                "1",
            ])
            .env("C3_NS_F31_DIR", &sync)
            .env("C3_NS_F31_HOME", &h.dir)
            .env("C3_NS_F31_BATCHES", batches)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let name = wait_for(&sync.join("ready-0"));
        assert!(
            name.starts_with(&format!("telemetry-not-spooled-{}-", child.id())),
            "{name}"
        );
        Producer {
            file: h.dir.join(name),
            child,
            sync,
            step: 0,
        }
    }

    /// Its next batch: appended when this returns (the producer still lives).
    fn next(&mut self) {
        self.step += 1;
        std::fs::write(self.sync.join(format!("go-{}", self.step)), "go").unwrap();
        wait_for(&self.sync.join(format!("ready-{}", self.step)));
    }

    /// Let it exit: gone when this returns.
    fn exit(mut self) {
        std::fs::write(self.sync.join(format!("go-{}", self.step + 1)), "go").unwrap();
        assert!(self.child.wait().unwrap().success());
    }
}

/// The contents of `p` once it is there (120 s at most).
fn wait_for(p: &Path) -> String {
    let t0 = std::time::Instant::now();
    loop {
        if let Ok(t) = std::fs::read_to_string(p) {
            return t;
        }
        assert!(
            t0.elapsed() < Duration::from_secs(120),
            "{} never came",
            p.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A flush while another handle holds the spool lock (the fold's 1 s wait runs out): its output and
/// the record it wrote.
fn flush_with_the_spool_lock_held(h: &H) -> (Output, Value) {
    let held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(h.dir.join("spool.lock"))
        .unwrap();
    held.lock().unwrap();
    let fl = h.flush(&[]);
    let last = h.last();
    drop(held);
    (fl, last)
}

/// The lines the record's fold notes count, summed.
fn folded_total(h: &H) -> u64 {
    let re =
        regex::Regex::new(r"^\S+ folded (\d+) not-spooled line\(s\) of \d+ gone producer\(s\)$")
            .unwrap();
    h.notes()
        .iter()
        .filter_map(|n| re.captures(n))
        .map(|c| c[1].parse::<u64>().unwrap())
        .sum()
}

fn lines_of(p: &Path) -> u64 {
    std::fs::read_to_string(p).unwrap().lines().count() as u64
}

/// (wave 3f, F31-1, RC1 of handoff 31) A producer LIVE with N lines at a flush that cannot take the
/// spool lock, then gone, then a fold under the lock: exactly N counted in total - the flush without
/// the lock sees NONE of them (`not_spooled_seen` 0, no `{name, bytes}` either), `--status` counts
/// them meanwhile, the fold counts them once in its note. With M lines appended between that flush
/// and the fold: N + M. And with the fold's own crash hooks (87: between the save and the deletes;
/// 88: between the deletes and the rewrite) and its restart: still N + M, once.
#[test]
fn f31_1_a_producer_live_at_a_flush_without_the_spool_lock_and_gone_at_the_fold_is_counted_once() {
    for (n, m, crash) in [(3u64, 0u64, ""), (3, 2, ""), (3, 2, "1"), (3, 2, "2")] {
        let case = format!("N {n}, M {m}, crash '{crash}'");
        let h = setup(&format!("f31-1-{n}-{m}-{crash}"));
        let batches = if m == 0 {
            n.to_string()
        } else {
            format!("{n},{m}")
        };
        let mut p = Producer::start(&h, &batches);
        assert_eq!(lines_of(&p.file), n, "{case}");
        assert_eq!(h.count().count, n, "{case}");
        // flush 1: the spool lock busy - no fold, and NO line seen
        let (fl1, last1) = flush_with_the_spool_lock_held(&h);
        assert_eq!(
            last1["not_spooled_seen"],
            0,
            "{case}: {} | {}",
            text(&fl1),
            h.last_text()
        );
        assert!(
            folded_entries(&last1).is_empty(),
            "{case}: {}",
            h.last_text()
        );
        assert_eq!(h.fold_notes(), 0, "{case}: {:?}", h.notes());
        assert!(p.file.exists(), "{case}");
        let st = h.status();
        assert!(
            st.contains(&format!(
                "\nnot spooled: {n} event(s) since the last flush - the latest "
            )),
            "{case}: {}",
            status_line(&st, "not spooled")
        );
        // M lines appended between the flush and the fold, then the producer exits
        if m > 0 {
            p.next();
        }
        let file = p.file.clone();
        p.exit();
        assert_eq!(lines_of(&file), n + m, "{case}");
        assert_eq!(h.count().count, n + m, "{case}");
        // flush 2: the fold under the lock (with a crash hook: then its restart)
        let fl2 = h.flush(&[("CODEX_CONSULT_TEST_FOLD_CRASH", crash)]);
        let want = match crash {
            "1" => 87,
            "2" => 88,
            _ => 0,
        };
        assert_eq!(fl2.status.code(), Some(want), "{case}: {}", text(&fl2));
        let note = format!(
            r"folded {} not-spooled line\(s\) of 1 gone producer\(s\)",
            n + m
        );
        assert_eq!(h.notes_like(&note), 1, "{case}: {:?}", h.notes());
        if crash == "1" {
            // saved, not deleted: the file is named with its length and counted nowhere meanwhile
            assert!(file.exists(), "{case}");
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            assert_eq!(
                folded_entries(&h.last()),
                vec![format!("{name}={}", len(&file))],
                "{case}"
            );
            assert_eq!(h.count().count, 0, "{case}");
        }
        if !crash.is_empty() {
            let restart = h.flush(&[]);
            assert_eq!(restart.status.code(), Some(0), "{case}: {}", text(&restart));
        }
        assert_eq!(h.fold_notes(), 1, "{case}: {:?}", h.notes());
        assert_eq!(h.notes_like(&note), 1, "{case}: {:?}", h.notes());
        let last2 = h.last();
        assert_eq!(last2["not_spooled_seen"], 0, "{case}: {}", h.last_text());
        assert!(
            folded_entries(&last2).is_empty(),
            "{case}: {}",
            h.last_text()
        );
        assert!(!file.exists(), "{case}");
        assert_eq!(h.count().count, 0, "{case}");
        // the total accounting: what flush 1 saw plus every fold note - exactly N + M
        assert_eq!(
            last1["not_spooled_seen"].as_u64().unwrap() + folded_total(&h),
            n + m,
            "{case}: {:?}",
            h.notes()
        );
        h.done();
    }
}

/// (wave 3f, F31-1) The flush without the spool lock moves the baseline NEITHER way: a fold under
/// the lock keeps the live producer's 2 lines (`not_spooled_seen` 2); one line later the flush
/// without the lock records 2 again - not 3 (that line is not seen: `--status` counts it and names
/// it), not 0 (the 2 the fold saw stay seen); one more line, the producer exits, and the next fold
/// counts all 4 ONCE in its note.
#[test]
fn f31_1_a_flush_without_the_spool_lock_keeps_the_last_folds_baseline() {
    let h = setup("f31-1-base");
    let mut p = Producer::start(&h, "2,1,1");
    let fl0 = h.flush(&[]);
    assert_eq!(fl0.status.code(), Some(0), "{}", text(&fl0));
    assert_eq!(h.last()["not_spooled_seen"], 2, "{}", h.last_text());
    assert_eq!(h.fold_notes(), 0, "{:?}", h.notes());
    assert_eq!(h.count().count, 0);
    p.next();
    assert_eq!(h.count().count, 1);
    let (fl1, last1) = flush_with_the_spool_lock_held(&h);
    assert_eq!(
        last1["not_spooled_seen"],
        2,
        "{} | {}",
        text(&fl1),
        h.last_text()
    );
    assert_eq!(h.fold_notes(), 0, "{:?}", h.notes());
    let st = h.status();
    let ns = status_line(&st, "not spooled");
    assert!(
        ns.starts_with("not spooled: 1 event(s) since the last flush - the latest ")
            && ns.contains(": batch 1 line 1 ("),
        "{ns}"
    );
    p.next();
    let file = p.file.clone();
    p.exit();
    assert_eq!(h.count().count, 2);
    let fl2 = h.flush(&[]);
    assert_eq!(fl2.status.code(), Some(0), "{}", text(&fl2));
    assert_eq!(
        h.notes_like(r"folded 4 not-spooled line\(s\) of 1 gone producer\(s\)"),
        1,
        "{:?}",
        h.notes()
    );
    assert_eq!(h.fold_notes(), 1);
    assert_eq!(folded_total(&h), 4);
    assert_eq!(h.last()["not_spooled_seen"], 0);
    assert!(!file.exists());
    assert_eq!(h.count().count, 0);
    h.done();
}

/// (F24-2, RC2) A crash between the save and the deletes (exit 87), then a last piece WITHOUT a
/// line end appended to the recorded file: the restarted flush counts it (one more fold note of 1
/// line) and deletes the file - never a silent zero.
#[test]
fn f24_2_a_tail_without_a_line_end_appended_to_a_recorded_file_is_counted() {
    let h = setup("f24-2");
    let name = "telemetry-not-spooled-999999-639000000000000000.ndjson";
    let gone = h.write(
        name,
        &(line("2026-10-01T10:00:00+02:00", "gone-1")
            + &line("2026-10-01T10:01:00+02:00", "gone-2")),
    );
    let len0 = len(&gone);
    let crash = h.flush(&[("CODEX_CONSULT_TEST_FOLD_CRASH", "1")]);
    assert_eq!(crash.status.code(), Some(87), "{}", text(&crash));
    assert_eq!(folded_entries(&h.last()), vec![format!("{name}={len0}")]);
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&gone)
            .unwrap();
        f.write_all(b"{\"time\":\"2026-10-01T10:05:00+02:00\",\"why\":\"tail\"}")
            .unwrap();
    }
    // --status counts complete lines only
    assert_eq!(h.count().count, 0);
    let fl = h.flush(&[]);
    assert_eq!(fl.status.code(), Some(0), "{}", text(&fl));
    assert_eq!(
        h.notes_like(r"folded 2 not-spooled line\(s\) of 1 gone producer\(s\)"),
        1,
        "{:?}",
        h.notes()
    );
    assert_eq!(
        h.notes_like(r"folded 1 not-spooled line\(s\) of 1 gone producer\(s\)"),
        1,
        "{:?}",
        h.notes()
    );
    assert_eq!(h.fold_notes(), 2);
    assert!(!gone.exists());
    assert!(folded_entries(&h.last()).is_empty());
    h.done();
}

/// (F24-3, RC3) A local forget paused after its cleanup listed the files: C3's producer writes
/// nothing meanwhile (and leaves no file); a file another producer makes after the listing (the
/// plugin's, an older C3 - it does not check) is removed by the cleanup's second listing. No
/// not-spooled file survives the forget; after it a count is written again.
#[test]
fn f24_3_a_count_made_during_a_local_forget_never_survives_it() {
    let h = setup("f24-3");
    h.write(
        "telemetry-not-spooled-999999-639000000000000000.ndjson",
        &line("2026-10-01T10:00:00+02:00", "gone"),
    );
    h.write(
        "telemetry-not-spooled.ndjson",
        &line("2026-09-01T10:00:00+02:00", "legacy"),
    );
    let (child, gate) = forget_at_the_gate(&h, &[]);
    let m: Value =
        serde_json::from_str(&std::fs::read_to_string(&h.paths.marker).unwrap()).unwrap();
    assert_eq!(m["pid"], child.id(), "the forget holds its marker");
    let e = notspooled::append(&h.paths, "during the forget").unwrap_err();
    assert!(
        e.contains("is not written while the local telemetry data is being deleted"),
        "{e}"
    );
    assert!(!h.own().exists(), "the refused append left no file");
    let late = "telemetry-not-spooled-999998-639000000000000002.ndjson";
    h.write(late, &line("2026-10-09T10:00:00+02:00", "late"));
    let (code, out) = open_the_gate(child, &gate);
    assert_eq!(code, Some(0), "{out}");
    assert!(out.contains("removed locally - "), "{out}");
    assert!(out.contains(late), "the late file is named: {out}");
    assert!(h.ns_names().is_empty(), "{:?}", h.ns_names());
    assert!(!h.paths.marker.exists());
    assert!(!h.dir.join("forget-pending.json").exists());
    notspooled::append(&h.paths, "after the forget").unwrap();
    assert!(h.own().exists());
    h.done();
}

/// (F24-4) `--status` names the latest line the last flush did not see: with
/// `not_spooled_seen` 1, a seen line dated 2030 before a new one dated 2020 names the 2020 one.
#[test]
fn f24_4_status_names_the_latest_unseen_line() {
    let h = setup("f24-4");
    h.write(
        "telemetry-not-spooled-999999-639000000000000000.ndjson",
        &(line("2030-01-01T00:00:00+00:00", "seen-2030")
            + &line("2020-01-01T00:00:00+00:00", "new-2020")),
    );
    std::fs::write(
        &h.paths.last,
        r#"{"time":"2026-10-09T10:00:00+02:00","result":"seeded","delivered":0,"kept":0,"dropped":0,"rejected":[],"http":null,"not_spooled_seen":1,"not_spooled_folded":[],"notes":[]}"#,
    )
    .unwrap();
    let st = h.status();
    assert!(
        st.contains("\nnot spooled: 1 event(s) since the last flush - the latest 2020-01-01T00:00:00+00:00: new-2020 ("),
        "{}",
        status_line(&st, "not spooled")
    );
    h.done();
}

/// (F24-5, P8) Under the plugin-home test hook the forgetting marker is the PLUGIN's
/// `<home>/telemetry-forgetting`: `--status` and the sender read it there (a living owner stops the
/// sender, a gone one is removed with a note in the plugin's `.last`), C3's own place is not read,
/// and a local forget writes its marker there (its pid and start ticks), refuses C3's producer
/// with it, and removes it last - with the plugin's not-spooled files and `.last`.
#[test]
fn f24_5_the_plugin_home_hook_puts_the_marker_at_the_plugin_place() {
    let h = setup("f24-5-hook");
    let home = h.home.to_string_lossy().to_string();
    let hook = [(notspooled::PLUGIN_HOME_VAR, home.as_str())];
    let pmk = h.home.join("telemetry-forgetting");
    let plast = h.home.join("telemetry-spool").join(".last");
    let me = std::process::id();
    let live = format!(
        "{{\"pid\":{me},\"start_ticks\":{},\"since\":\"2026-10-09T01:00:00+02:00\"}}\n",
        notspooled::own_start_ticks()
    );
    // a living owner's marker at the plugin place
    std::fs::write(&pmk, &live).unwrap();
    let st = text(&h.c3(&["telemetry", "--status"], &hook));
    let re = regex::Regex::new(&format!(
        r"(?m)^forgetting : the marker {} is there - its owner pid {me} lives",
        regex::escape(&pmk.display().to_string())
    ))
    .unwrap();
    assert!(re.is_match(&st), "{}", status_line(&st, "forgetting"));
    let fl = text(&h.c3(&["telemetry", "--flush", "--telemetry", "on"], &hook));
    assert!(
        fl.contains("skipped - c3 telemetry --forget --local is deleting")
            && fl.contains(&pmk.display().to_string()),
        "{fl}"
    );
    assert!(pmk.exists());
    // C3's own place is not read under the hook
    std::fs::remove_file(&pmk).unwrap();
    std::fs::write(&h.paths.marker, &live).unwrap();
    let st = text(&h.c3(&["telemetry", "--status"], &hook));
    assert!(!st.contains("\nforgetting : the marker"), "{st}");
    let fl = text(&h.c3(&["telemetry", "--flush", "--telemetry", "on"], &hook));
    assert!(!fl.contains("is deleting"), "{fl}");
    std::fs::remove_file(&h.paths.marker).unwrap();
    // a gone owner's marker at the plugin place: the sender removes it, the note in the plugin's .last
    std::fs::write(&pmk, "{\"pid\":999999,\"since\":\"x\"}\n").unwrap();
    let fl = h.c3(&["telemetry", "--flush", "--telemetry", "on"], &hook);
    assert!(!pmk.exists(), "{}", text(&fl));
    let pl: Value = serde_json::from_str(&std::fs::read_to_string(&plast).unwrap()).unwrap();
    assert!(
        pl["notes"].as_array().unwrap().iter().any(|n| n
            .as_str()
            .unwrap_or("")
            .contains("removed the forgetting marker of pid 999999 (gone) since x")),
        "{pl}"
    );
    assert!(
        !h.paths.last.exists(),
        "C3's own record is not written under the hook"
    );
    // the forget's own marker at the plugin place
    let pgone = h
        .home
        .join("telemetry-not-spooled-999999-639000000000000000.ndjson");
    std::fs::write(&pgone, line("2026-10-01T10:00:00+02:00", "plugin-place")).unwrap();
    let (child, gate) = forget_at_the_gate(&h, &hook);
    let m: Value = serde_json::from_str(&std::fs::read_to_string(&pmk).unwrap()).unwrap();
    assert_eq!(m["pid"], child.id());
    assert!(m["start_ticks"].as_i64().is_some_and(|t| t > 0), "{m}");
    assert!(!h.paths.marker.exists(), "not at C3's place");
    let pp = LocalPaths::plugin_home(&h.home, &h.dir);
    assert!(notspooled::append(&pp, "during").is_err());
    let (code, out) = open_the_gate(child, &gate);
    assert_eq!(code, Some(0), "{out}");
    assert!(!pmk.exists(), "removed last");
    assert!(!plast.exists(), "{out}");
    assert!(!pgone.exists(), "{out}");
    h.done();
}

/// (F24-5, P8) Without the hook - test mode on and the variable unset, or the variable set with
/// test mode off - every file is under C3's own root `<codex home>/c3/telemetry/`: the fold's
/// record and files, the forget's marker; nothing appears at the plugin's places.
#[test]
fn f24_5_without_the_hook_every_file_is_under_c3s_own_root() {
    let h = setup("f24-5-prod");
    let home = h.home.to_string_lossy().to_string();
    let plugin_places = || -> Vec<String> {
        std::fs::read_dir(&h.home)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n != "c3")
            .collect()
    };
    let gone = h.write(
        "telemetry-not-spooled-999999-639000000000000000.ndjson",
        &line("2026-10-01T10:00:00+02:00", "gone-1"),
    );
    let fl = h.flush(&[]);
    assert_eq!(fl.status.code(), Some(0), "{}", text(&fl));
    assert_eq!(
        h.notes_like(r"folded 1 not-spooled line\(s\) of 1 gone producer\(s\)"),
        1
    );
    assert!(!gone.exists());
    assert!(plugin_places().is_empty(), "{:?}", plugin_places());
    // the variable set but test mode off: ignored
    let gone2 = h.write(
        "telemetry-not-spooled-999999-639000000000000001.ndjson",
        &(line("2026-10-02T10:00:00+02:00", "gone-2")
            + &line("2026-10-02T10:01:00+02:00", "gone-3")),
    );
    let fl = h.c3(
        &["telemetry", "--flush", "--telemetry", "on"],
        &[
            (notspooled::PLUGIN_HOME_VAR, &home),
            ("CODEX_CONSULT_TEST_MODE", ""),
        ],
    );
    assert_eq!(fl.status.code(), Some(0), "{}", text(&fl));
    assert_eq!(
        h.notes_like(r"folded 2 not-spooled line\(s\) of 1 gone producer\(s\)"),
        1,
        "{:?}",
        h.notes()
    );
    assert!(!gone2.exists());
    assert!(plugin_places().is_empty(), "{:?}", plugin_places());
    // the forget's marker at C3's root
    notspooled::append(&h.paths, "own").unwrap();
    let (child, gate) = forget_at_the_gate(&h, &[]);
    let m: Value =
        serde_json::from_str(&std::fs::read_to_string(&h.paths.marker).unwrap()).unwrap();
    assert_eq!(m["pid"], child.id());
    assert!(!h.home.join("telemetry-forgetting").exists());
    let (code, out) = open_the_gate(child, &gate);
    assert_eq!(code, Some(0), "{out}");
    assert!(!h.paths.marker.exists());
    assert!(h.ns_names().is_empty(), "{:?}", h.ns_names());
    assert!(plugin_places().is_empty(), "{:?}", plugin_places());
    h.done();
}

/// (F24-6) The rewrite after the fold's deletes fails (test hook
/// `CODEX_CONSULT_TEST_FOLD_REWRITE_FAIL=1`): the flush says so, the record still names the
/// deleted files - `--status` is unaffected -, and the next flush counts everything new exactly once
/// (a new legacy generation staged under a fresh name, a new gone producer, and a file recreated
/// SHORTER under a stale name - folded afresh, E24) and drops the stale names.
#[test]
fn f24_6_a_failed_rewrite_after_the_deletes_is_said_and_loses_no_count() {
    let h = setup("f24-6");
    notspooled::append(&h.paths, "live").unwrap();
    let gname = "telemetry-not-spooled-999999-639000000000000000.ndjson";
    let gone = h.write(
        gname,
        &(line("2026-10-01T10:00:00+02:00", "gone-1")
            + &line("2026-10-01T10:01:00+02:00", "gone-2")),
    );
    h.write(
        "telemetry-not-spooled.ndjson",
        &line("2026-09-01T10:00:00+02:00", "legacy-1"),
    );
    let fl = h.flush(&[("CODEX_CONSULT_TEST_FOLD_REWRITE_FAIL", "1")]);
    let out = text(&fl);
    assert_eq!(fl.status.code(), Some(0), "{out}");
    assert!(
        out.contains("; warning: ")
            && out.contains("could not be rewritten after the fold's deletes")
            && out.contains("it still names 2 deleted file(s)"),
        "{out}"
    );
    let stale = folded_entries(&h.last());
    assert_eq!(stale.len(), 2, "{stale:?}");
    assert!(!gone.exists());
    assert!(h.staged().is_empty());
    assert_eq!(
        h.notes_like(r"folded 3 not-spooled line\(s\) of 2 gone producer\(s\)"),
        1
    );
    assert_eq!(h.count().count, 0);
    // new lines: the legacy name again, a new gone producer, the stale producer name recreated shorter
    h.write(
        "telemetry-not-spooled.ndjson",
        &line("2026-09-02T10:00:00+02:00", "legacy-2"),
    );
    h.write(
        "telemetry-not-spooled-999997-639000000000000000.ndjson",
        &line("2026-10-02T10:00:00+02:00", "gone-new"),
    );
    h.write(gname, &line("2026-10-03T10:00:00+02:00", "x"));
    assert!(len(&gone) < len0_of(&stale, gname));
    assert_eq!(h.count().count, 3);
    let fl2 = h.flush(&[]);
    assert_eq!(fl2.status.code(), Some(0), "{}", text(&fl2));
    assert!(!text(&fl2).contains("warning"), "{}", text(&fl2));
    assert_eq!(
        h.notes_like(r"folded 3 not-spooled line\(s\) of 3 gone producer\(s\)"),
        1,
        "{:?}",
        h.notes()
    );
    assert_eq!(h.fold_notes(), 2);
    assert!(folded_entries(&h.last()).is_empty(), "{}", h.last_text());
    assert_eq!(h.last()["not_spooled_seen"], 1);
    assert_eq!(
        h.ns_names(),
        vec![h.own().file_name().unwrap().to_string_lossy().to_string()]
    );
    assert_eq!(h.count().count, 0);
    h.done();
}

/// The recorded bytes of `name` in `folded_entries` output (`name=bytes`).
fn len0_of(entries: &[String], name: &str) -> u64 {
    entries
        .iter()
        .find_map(|e| e.strip_prefix(&format!("{name}=")))
        .and_then(|b| b.parse().ok())
        .unwrap()
}
