//! `c3 telemetry --backfill-ratings [--dry-run]` (R24; 0.6.1 F06-1, F06-2) - the plugin's
//! `Invoke-TelemetryBackfillRatings`.
//!
//! Every mark of every task of the current repository (`<collab>/<task>/findings.json`
//! `ratings`) without `telemetry_sent` gets its rating event spooled ONCE: the ledger entry looked
//! up as `--rate` recorded it (`consult_id`, else `n`; none: the mark is SKIPPED and counted - a
//! reviewer is never guessed; a mark that is not `yes|partly|no` or whose `when` does not parse is
//! skipped too), the event built from the MARK - its own `when` (client_time), `consult_when`
//! (age_days), saved `judge` and `rating_rev` - spooled with up to 5 s, and `telemetry_sent`
//! written into the mark under the task's store commit (the write lock, findings.json re-read
//! under it, written once at the end). A mark without a saved judge (rated before 0.6.1) takes the
//! consultation's own coordinator (`consult_coordinator`) or `unknown` - never this process's
//! `CODEX_CONSULT_COORDINATOR`. A spool failure stops that task's remaining marks (they stay
//! unsent: the next run sends them). `--dry-run` reads without a lock, prints per event the
//! classes it would send and writes nothing.

use std::path::Path;
use std::time::Duration;

use c3_core::findings::{FindingsFile, Rating};
use c3_core::ledger::{LedgerEntry, SessionsFile};
use c3_core::store::{write_text_atomic, EvidenceStore, FilesStore};
use c3_core::task_slug::{is_slug, TaskSlug};

use crate::telemetry::classes::{self, Judge};
use crate::telemetry::event::{parse_when, RatingEvent, RatingInput};
use crate::telemetry::Switch;

const TOOL: &str = "codex-telemetry";
const MARKS: [&str; 3] = ["yes", "partly", "no"];

/// `Find-RatingLedgerEntry`: the ledger entry a mark rates - as `--rate` recorded it: the entry
/// whose `consult_id` equals the mark's (case-insensitive), or - a mark without one (recorded
/// before wave 26) - the entry whose `n` equals the mark's. `None` when none (never a guess).
pub fn find_rating_ledger_entry<'a>(
    consults: &'a [LedgerEntry],
    mark: &Rating,
) -> Option<&'a LedgerEntry> {
    let cid = mark.consult_id.trim();
    let mut found = None;
    for c in consults {
        if !cid.is_empty() {
            if c.consult_id.eq_ignore_ascii_case(cid) {
                found = Some(c);
            }
        } else if c.n == mark.n {
            found = Some(c);
        }
    }
    found
}

/// The judge a backfilled mark's event carries: the one the mark saved, else (a mark rated before
/// 0.6.1) the consultation's own coordinator or unknown - never the backfilling process's actor.
pub fn backfill_judge(
    mark: &Rating,
    entry: &LedgerEntry,
    roster: &c3_core::roster::Roster,
) -> Judge {
    classes::mark_judge(mark.judge.as_ref())
        .unwrap_or_else(|| classes::close_judge(&classes::resolve_judge(entry, None, Some(roster))))
}

/// Run the backfill over every task of `collab_root`; returns the exit code (0 done, 1 refused or
/// something not spooled).
pub fn run(collab_root: &Path, switch: &Switch, dry_run: bool) -> i32 {
    if !switch.on {
        println!(
            "{TOOL}: telemetry is off ({}): -BackfillRatings sends nothing and writes nothing - switch it on (CODEX_CONSULT_TELEMETRY unset or on, or -Telemetry on) to backfill.",
            switch.source
        );
        return 1;
    }
    let verb = if dry_run { "would send" } else { "sent" };
    let (mut t_sent, mut t_already, mut t_skipped, mut t_failed, mut t_tasks) = (0, 0, 0, 0, 0);
    // the reviewer roster for the judge's label lookup, read once at the first mark that needs it
    let mut judge_roster: Option<c3_core::roster::Roster> = None;
    let mut task_names: Vec<String> = std::fs::read_dir(collab_root)
        .map(|rd| {
            rd.flatten()
                .filter(|d| d.path().is_dir())
                .filter_map(|d| d.file_name().to_str().map(str::to_string))
                .filter(|n| is_slug(n) && collab_root.join(n).join("findings.json").is_file())
                .collect()
        })
        .unwrap_or_default();
    task_names.sort();
    let store = FilesStore::new(collab_root.to_path_buf());
    for task in task_names {
        let task_dir = collab_root.join(&task);
        let findings_path = task_dir.join("findings.json");
        let sessions_path = task_dir.join("sessions.json");
        // a first look without a lock: a task without marks is not listed, one whose marks are all
        // sent takes no lock
        let peek = match read_findings(&findings_path) {
            Ok(f) => f,
            Err(why) => {
                println!("{TOOL}: {task}: not processed - {why}");
                t_failed += 1;
                continue;
            }
        };
        let marks = peek.ratings.as_deref().unwrap_or(&[]);
        if marks.is_empty() {
            continue;
        }
        t_tasks += 1;
        let needs_lock = !dry_run && marks.iter().any(|m| m.telemetry_sent.is_none());
        let mut lock = None;
        let mut findings = peek;
        if needs_lock {
            let slug = match TaskSlug::new(task.clone()) {
                Ok(s) => s,
                Err(e) => {
                    println!("{TOOL}: {task}: not processed - {e}");
                    t_failed += 1;
                    continue;
                }
            };
            match store.take_write_lock(&slug) {
                Ok(l) => lock = Some(l),
                Err(e) => {
                    println!(
                        "{TOOL}: {task}: not processed - the write lock was not acquired ({e})"
                    );
                    t_failed += 1;
                    continue;
                }
            }
            findings = match read_findings(&findings_path) {
                Ok(f) => f,
                Err(why) => {
                    println!("{TOOL}: {task}: not processed - {why}");
                    t_failed += 1;
                    continue;
                }
            };
        }
        let consults: Vec<LedgerEntry> = std::fs::read(&sessions_path)
            .ok()
            .and_then(|b| SessionsFile::read(&b).ok())
            .map(|s| s.codex.consults)
            .unwrap_or_default();
        let (mut sent, mut already, mut skipped, mut failed) = (0, 0, 0, 0);
        let mut why = String::new();
        let mut changed = false;
        for m in findings.ratings.iter_mut().flatten() {
            if m.telemetry_sent.is_some() {
                already += 1;
                continue;
            }
            let useful = m.useful.trim().to_lowercase();
            let rated_at = parse_when(&m.when);
            let entry = find_rating_ledger_entry(&consults, m);
            let (Some(entry), Some(rated_at)) = (entry, rated_at) else {
                skipped += 1;
                continue;
            };
            if !MARKS.contains(&useful.as_str()) {
                skipped += 1;
                continue;
            }
            let roster = judge_roster.get_or_insert_with(|| {
                crate::providers::read_reviewer_roster().unwrap_or_default()
            });
            let judge = backfill_judge(m, entry, roster);
            // (F06-2) the mark's own revision - never re-allocated here
            let rev = m.rating_rev_value();
            let consult_when = m.consult_when.clone().flatten();
            let input = RatingInput {
                entry,
                mark: &useful,
                rated_at,
                consult_when: consult_when.as_deref(),
                judge: &judge,
                rating_rev: rev,
            };
            if dry_run {
                let ev = RatingEvent::from_rating(&input, "");
                let d = &ev.details;
                println!(
                    "{TOOL}: would send: {} / {} ({}), purpose {}, mark {}, age_days {}, client_time {}, judge {} / {} ({})",
                    d.provider,
                    d.model,
                    d.engine,
                    d.purpose,
                    d.mark,
                    d.age_days,
                    ev.client_time,
                    d.judge.provider,
                    d.judge.model,
                    d.judge.source
                );
                sent += 1;
                continue;
            }
            if !why.is_empty() {
                failed += 1;
                continue;
            }
            match crate::telemetry::record_rating(&input, Duration::from_secs(5)) {
                Ok(_) => {
                    m.telemetry_sent =
                        Some(serde_json::Value::from(chrono::Utc::now().timestamp()));
                    changed = true;
                    sent += 1;
                }
                Err(e) => {
                    why = e.to_string();
                    crate::telemetry::note_not_spooled(&why);
                    failed += 1;
                }
            }
        }
        if changed {
            let written = findings
                .to_bytes()
                .map_err(|e| e.to_string())
                .and_then(|b| write_text_atomic(&findings_path, &b).map_err(|e| e.to_string()));
            if let Err(e) = written {
                // the events are spooled but the marks keep no telemetry_sent: a next run would
                // send them once more
                println!("{TOOL}: {task}: warning: the marks' telemetry_sent could not be written ({e}) - a next run would send them once more");
            }
        }
        drop(lock);
        t_sent += sent;
        t_already += already;
        t_skipped += skipped;
        t_failed += failed;
        let mut line =
            format!("{TOOL}: {task}: {verb} {sent}, already {already}, skipped {skipped}");
        if failed > 0 {
            line.push_str(&format!(
                "; not spooled {failed} ({why}) - they stay unsent"
            ));
        }
        println!("{line}");
    }
    let mut total =
        format!("{TOOL}: total: {verb} {t_sent}, already {t_already}, skipped {t_skipped}");
    if t_failed > 0 {
        total.push_str(&format!(", not spooled or not processed {t_failed}"));
    }
    total.push_str(&format!(
        " in {t_tasks} task(s) with marks under {}",
        collab_root.display()
    ));
    if t_skipped > 0 {
        total.push_str(" (skipped: no ledger entry for the mark, or a mark that is not yes|partly|no or has no time)");
    }
    println!("{total}");
    if dry_run {
        println!("{TOOL}: dry run - nothing was spooled or written.");
    } else if t_sent > 0 {
        // (wave 6) the plugin's detached sender and its two lines
        match crate::telemetry::start_sender() {
            Ok(()) => println!("{TOOL}: the sender started (detached) - c3 telemetry --status shows the result."),
            Err(why) => println!("{TOOL}: the sender did not start ({why}) - the next consultation's sender, or c3 telemetry --flush, delivers the spool."),
        }
    }
    if t_failed > 0 {
        1
    } else {
        0
    }
}

/// `findings.json` as the typed store (a missing file is a store without marks).
fn read_findings(path: &Path) -> Result<FindingsFile, String> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(FindingsFile::default()),
        Err(e) => return Err(format!("'{}' could not be read ({e})", path.display())),
    };
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}');
    serde_json::from_str(text).map_err(|e| {
        format!(
            "'{}' does not parse ({})",
            path.display(),
            c3_core::one_line(&e.to_string())
        )
    })
}
