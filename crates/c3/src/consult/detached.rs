//! The detached-run status file and its readers (wave 25, R12): a port of the pure-logic
//! parts of `codex-consult-detached.ps1` — `Get-DetachedPaths`, `ConvertTo-DetachedRecord`,
//! `Read-DetachedStatus`, `Get-DetachedJudgement`, `Format-DetachedSpan`,
//! `Format-DetachedListLine`, `Get-DetachedPhrase` — plus `Get-DetachedBudget`.
//!
//! A detached run's state lives in `<task>/.consult.detached-<id8>.status.json` (atomic
//! replace); its console output in the sibling `.log`. Both names start with `.consult.` so the
//! collab snapshot leaves them out. Readers judge a record that is not `done` by its background's
//! liveness (pid + start time, on this host).

use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::liveness::proc::pid_alive;

pub const STATUS_VERSION: i64 = 1;
/// A `starting` record without a pid reads "starting" this long, then "never started".
pub const START_GRACE_SEC: i64 = 60;
/// `-Status -Prune` removes done/died runs after this many days.
pub const PRUNE_DAYS: i64 = 7;

/// A member's state (`$script:DetachedMemberStates`).
pub const MEMBER_STATES: &[&str] = &[
    "pending",
    "running",
    "usable",
    "failed",
    "skipped",
    "killed",
    "blocked",
    "commit_blocked",
    "orphan",
];
/// The member states that count as "finished".
pub const MEMBER_DONE: &[&str] = &[
    "usable",
    "failed",
    "killed",
    "blocked",
    "commit_blocked",
    "orphan",
];

/// One entry of a record's `members[]` (every field, in canonical order).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DetachedMember {
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub lineage: String,
    #[serde(default = "default_pending")]
    pub state: String,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub wall_seconds: Option<f64>,
    #[serde(default)]
    pub n: Option<i64>,
    #[serde(default)]
    pub handoff: String,
    #[serde(default)]
    pub reply: String,
}

fn default_pending() -> String {
    "pending".to_string()
}

fn default_run() -> String {
    "run".to_string()
}

/// The detached-run status record (`status_version 1`), fields in canonical order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetachedRecord {
    #[serde(default = "default_version")]
    pub status_version: i64,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub id8: String,
    #[serde(default)]
    pub task: String,
    #[serde(default = "default_run")]
    pub kind: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub exit: Option<i64>,
    #[serde(default)]
    pub started: Option<String>,
    #[serde(default)]
    pub updated: Option<String>,
    #[serde(default)]
    pub finished: Option<String>,
    #[serde(default)]
    pub wall_seconds: Option<f64>,
    #[serde(default)]
    pub pid: Option<i64>,
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub budget_sec: i64,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub reply_name: String,
    #[serde(default)]
    pub brief: String,
    #[serde(default)]
    pub members: Vec<DetachedMember>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub log: String,
    /// The foreground's base64 CLIXML-equivalent argument blob (only in `starting`); the
    /// background's self-reports drop it (`None`).
    #[serde(default)]
    pub args: Option<String>,
}

fn default_version() -> i64 {
    STATUS_VERSION
}

impl Default for DetachedRecord {
    fn default() -> Self {
        DetachedRecord {
            status_version: STATUS_VERSION,
            id: String::new(),
            id8: String::new(),
            task: String::new(),
            kind: "run".into(),
            state: String::new(),
            exit: None,
            started: None,
            updated: None,
            finished: None,
            wall_seconds: None,
            pid: None,
            start_time: None,
            host: String::new(),
            budget_sec: 0,
            purpose: String::new(),
            reply_name: String::new(),
            brief: String::new(),
            members: Vec::new(),
            summary: String::new(),
            log: String::new(),
            args: None,
        }
    }
}

/// The files of a detached run.
pub struct DetachedPaths {
    pub id8: String,
    pub status: PathBuf,
    pub log: PathBuf,
    pub prompt: PathBuf,
}

/// `Get-DetachedPaths`: the status/log/prompt files for a detach id (`id8` = the first 8 hex
/// digits, lowercased).
pub fn detached_paths(task_dir: &Path, id: &str) -> DetachedPaths {
    let mut id8: String = id.replace('-', "");
    if id8.len() > 8 {
        id8.truncate(8);
    }
    let id8 = id8.to_lowercase();
    DetachedPaths {
        status: task_dir.join(format!(".consult.detached-{id8}.status.json")),
        log: task_dir.join(format!(".consult.detached-{id8}.log")),
        prompt: task_dir.join(format!(".consult.detached-{id8}.prompt.txt")),
        id8,
    }
}

/// `Read-DetachedStatus`: `(record, error)`. `Ok(Some)` = a usable record; `Ok(None)` = the file
/// is absent; `Err` = present but unusable (empty/unparseable/not an object/no id/bad state).
pub fn read_detached_status(path: &Path) -> Result<Option<DetachedRecord>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let why = |w: &str| format!("the status file '{}' cannot be used: {w}", path.display());
    if text.trim().is_empty() {
        return Err(why("it is empty or could not be read"));
    }
    let v: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return Err(why(&format!("it does not parse: {e}"))),
    };
    if !v.is_object() {
        return Err(why("it is not a JSON object"));
    }
    let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
    if id.is_empty() {
        return Err(why("it names no id"));
    }
    let state = v.get("state").and_then(|x| x.as_str()).unwrap_or("");
    if !["starting", "running", "done"].contains(&state) {
        return Err(why(&format!(
            "its state '{state}' is not one of starting|running|done"
        )));
    }
    match serde_json::from_value::<DetachedRecord>(v) {
        Ok(r) => Ok(Some(r)),
        Err(e) => Err(why(&format!("it does not parse: {e}"))),
    }
}

/// Serialize a record to the on-disk byte stream `Write-JsonFile` would produce.
pub fn record_to_bytes(record: &DetachedRecord) -> Result<Vec<u8>, serde_json::Error> {
    c3_core::ps_json::to_ps_json_bytes(record)
}

fn parse_when(s: &str) -> Option<DateTime<FixedOffset>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt);
    }
    let alt = s.replacen(' ', "T", 1);
    DateTime::parse_from_rfc3339(&alt).ok()
}

fn is_not_started(m: &DetachedMember) -> bool {
    m.state == "skipped" && m.outcome.starts_with("not started")
}

/// `Format-DetachedSpan`.
pub fn format_span(secs: f64) -> String {
    let secs = secs.max(0.0);
    if secs < 90.0 {
        return format!("{} s", secs.floor() as i64);
    }
    let mins = secs / 60.0;
    if mins < 90.0 {
        return format!("{} min", mins.floor() as i64);
    }
    let hours = secs / 3600.0;
    if hours < 48.0 {
        let h = hours.floor() as i64;
        let m = ((secs - (h as f64) * 3600.0) / 60.0).floor() as i64;
        return format!("{h} h {m:02} min");
    }
    let days = (secs / 86400.0).floor() as i64;
    let h = ((secs - (days as f64) * 86400.0) / 3600.0).floor() as i64;
    format!("{days} d {h:02} h")
}

/// The verdict on one detached run.
#[derive(Debug, Clone)]
pub struct Judgement {
    /// `done` | `running` | `elsewhere` | `starting` | `died` | `never-started` | `unreadable`.
    pub state: String,
    pub exit: i32,
    pub text: String,
    pub finished: i64,
    pub of: i64,
}

/// `Get-DetachedJudgement` (D5, D6, D11). `problem` is a non-empty read error (unreadable);
/// `record` is `None` when the file was absent. `machine_name` is this host's name (for the
/// other-host check).
pub fn judgement(
    record: Option<&DetachedRecord>,
    problem: &str,
    now: DateTime<Utc>,
    machine_name: &str,
) -> Judgement {
    let record = match (problem.is_empty(), record) {
        (true, Some(r)) => r,
        _ => {
            let text = if problem.is_empty() {
                "no status record".to_string()
            } else {
                problem.to_string()
            };
            return Judgement {
                state: "unreadable".into(),
                exit: 1,
                text,
                finished: 0,
                of: 0,
            };
        }
    };
    let members: Vec<&DetachedMember> = record.members.iter().collect();
    let of = members
        .iter()
        .filter(|m| m.state != "skipped" || is_not_started(m))
        .count() as i64;
    let finished = members
        .iter()
        .filter(|m| MEMBER_DONE.contains(&m.state.as_str()) || is_not_started(m))
        .count() as i64;
    let started = record.started.as_deref().and_then(parse_when);
    let started_text = record
        .started
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("(unknown)");

    if record.state == "done" {
        let exit_text = record
            .exit
            .map(|c| c.to_string())
            .unwrap_or_else(|| "unknown".into());
        let ok = record.exit == Some(0);
        return Judgement {
            state: "done".into(),
            exit: if ok { 0 } else { 1 },
            text: format!("done, exit {exit_text}"),
            finished,
            of,
        };
    }

    let bg_pid = record.pid.unwrap_or(0);
    if bg_pid <= 0 {
        let age_secs = started
            .map(|s| (now - s.with_timezone(&Utc)).num_seconds())
            .unwrap_or(i64::MAX);
        if age_secs < START_GRACE_SEC {
            return Judgement {
                state: "starting".into(),
                exit: 2,
                text: format!(
                    "starting - the background has not reported yet (started {} ago)",
                    format_span(age_secs.max(0) as f64)
                ),
                finished,
                of,
            };
        }
        return Judgement {
            state: "never-started".into(),
            exit: 1,
            text: format!(
                "never started - no background process reported within {START_GRACE_SEC} s of {started_text}; if one starts late it still reports and the run reads running again (its log may say why)"
            ),
            finished,
            of,
        };
    }

    if !record.host.is_empty() && !record.host.eq_ignore_ascii_case(machine_name) {
        return Judgement {
            state: "elsewhere".into(),
            exit: 2,
            text: format!(
                "running on host {} since {started_text} (pid {bg_pid}) - its liveness cannot be checked from this host",
                record.host
            ),
            finished,
            of,
        };
    }

    let start_time = record.start_time.clone().unwrap_or_default();
    if pid_alive(bg_pid as u32, &start_time) {
        let since = started
            .map(|s| {
                format!(
                    " ({})",
                    format_span((now - s.with_timezone(&Utc)).num_seconds().max(0) as f64)
                )
            })
            .unwrap_or_default();
        return Judgement {
            state: "running".into(),
            exit: 2,
            text: format!(
                "running since {started_text}{since}, {finished} of {of} members finished"
            ),
            finished,
            of,
        };
    }
    Judgement {
        state: "died".into(),
        exit: 1,
        text: format!(
            "died - its background process (pid {bg_pid}) is gone without a final status, {finished} of {of} members finished; its recovery records are judged by the next run of the task as usual"
        ),
        finished,
        of,
    }
}

/// `Format-DetachedListLine`: the `findings --list` line for a not-done run (`""` for a done
/// one).
pub fn list_line(
    record: Option<&DetachedRecord>,
    error: &str,
    id8: &str,
    task: &str,
    now: DateTime<Utc>,
    machine_name: &str,
) -> String {
    let j = judgement(record, error, now, machine_name);
    if j.state == "done" {
        return String::new();
    }
    format!(
        "detached {id8}: {} (codex-consult.ps1 -Task {task} -Status -Id {id8})",
        j.text
    )
}

/// `Get-DetachedBudget` (D4): `-Wait`'s default timeout. `groups` = `(limit, member_count)` per
/// endpoint group; `guard_of` maps a roster position to its kill guard; the per-group max guard
/// is derived from `positions` + `guard_of`. Here we take each group's `(limit, positions)` and
/// its max guard directly for a self-contained port.
pub fn detached_budget(
    groups: &[(i64, Vec<i64>)],
    guard_of: &dyn Fn(i64) -> i64,
    cap: i64,
    slack: i64,
) -> i64 {
    let mut by_groups = 0i64;
    let mut longest = 0i64;
    let mut count = 0i64;
    for (limit, positions) in groups {
        if positions.is_empty() {
            continue;
        }
        let g_max = positions.iter().map(|p| guard_of(*p)).max().unwrap_or(0);
        let limit = (*limit).max(1);
        let waves = (positions.len() as f64 / limit as f64).ceil() as i64;
        if waves * g_max > by_groups {
            by_groups = waves * g_max;
        }
        if g_max > longest {
            longest = g_max;
        }
        count += positions.len() as i64;
    }
    let mut by_cap = 0i64;
    if cap > 0 && count > 0 {
        by_cap = ((count as f64 / cap as f64).ceil() as i64) * longest;
    }
    by_groups.max(by_cap) + slack
}

/// `Get-PanelMemberGuard`: a panel member's kill guard.
pub fn member_guard(timeout_sec: i64, continue_sec: i64, repair: bool, denial_retry: bool) -> i64 {
    let mut g = timeout_sec + 60 + 120;
    if repair {
        g += timeout_sec.min(300);
    }
    if denial_retry {
        g += timeout_sec.min(300);
    }
    if continue_sec > 0 {
        g += continue_sec;
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec_json(json: serde_json::Value) -> DetachedRecord {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn paths_lowercase_and_truncate_id8() {
        let p = detached_paths(
            Path::new("/collab/my-task"),
            "ABCDEF12-3456-7890-abcd-ef1234567890",
        );
        assert_eq!(p.id8, "abcdef12");
        assert!(p
            .status
            .to_string_lossy()
            .ends_with(".consult.detached-abcdef12.status.json"));
        assert!(p
            .log
            .to_string_lossy()
            .ends_with(".consult.detached-abcdef12.log"));
    }

    #[test]
    fn record_round_trips_through_ps_json() {
        let mut r = DetachedRecord {
            id: "abcdef12-0000".into(),
            id8: "abcdef12".into(),
            task: "my-task".into(),
            kind: "panel".into(),
            state: "running".into(),
            pid: Some(4321),
            host: "HOST".into(),
            budget_sec: 1920,
            purpose: "framing".into(),
            ..Default::default()
        };
        r.members.push(DetachedMember {
            position: 1,
            lineage: "openai :: gpt-6".into(),
            state: "usable".into(),
            wall_seconds: Some(12.5),
            n: Some(3),
            handoff: "04".into(),
            ..Default::default()
        });
        let bytes = record_to_bytes(&r).unwrap();
        let back: DetachedRecord = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.id8, "abcdef12");
        assert_eq!(back.kind, "panel");
        assert_eq!(back.members.len(), 1);
        assert_eq!(back.members[0].n, Some(3));
        assert_eq!(back.pid, Some(4321));
        // The canonical order puts status_version first.
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.trim_start().starts_with('{'));
        let sv = text.find("status_version").unwrap();
        let idpos = text.find("\"id\"").unwrap();
        assert!(sv < idpos, "status_version precedes id");
    }

    #[test]
    fn judgement_done_maps_exit() {
        let r = rec_json(serde_json::json!({"id":"x","state":"done","exit":0}));
        let j = judgement(Some(&r), "", Utc::now(), "HOST");
        assert_eq!((j.state.as_str(), j.exit), ("done", 0));
        let r = rec_json(serde_json::json!({"id":"x","state":"done","exit":1}));
        let j = judgement(Some(&r), "", Utc::now(), "HOST");
        assert_eq!((j.state.as_str(), j.exit), ("done", 1));
        // done with a null exit is a failure.
        let r = rec_json(serde_json::json!({"id":"x","state":"done","exit":null}));
        let j = judgement(Some(&r), "", Utc::now(), "HOST");
        assert_eq!(j.exit, 1);
    }

    #[test]
    fn judgement_starting_then_never_started() {
        let now = Utc::now();
        let recent = (now - chrono::Duration::seconds(10)).to_rfc3339();
        let r = rec_json(serde_json::json!({"id":"x","state":"starting","started":recent}));
        let j = judgement(Some(&r), "", now, "HOST");
        assert_eq!((j.state.as_str(), j.exit), ("starting", 2));
        let old = (now - chrono::Duration::seconds(120)).to_rfc3339();
        let r = rec_json(serde_json::json!({"id":"x","state":"starting","started":old}));
        let j = judgement(Some(&r), "", now, "HOST");
        assert_eq!((j.state.as_str(), j.exit), ("never-started", 1));
    }

    #[test]
    fn judgement_elsewhere_is_never_judged() {
        let now = Utc::now();
        let r = rec_json(serde_json::json!({
            "id":"x","state":"running","pid":999999,"host":"OTHER-HOST",
            "started": now.to_rfc3339()
        }));
        let j = judgement(Some(&r), "", now, "THIS-HOST");
        assert_eq!((j.state.as_str(), j.exit), ("elsewhere", 2));
    }

    #[test]
    fn judgement_died_when_pid_gone_on_this_host() {
        let now = Utc::now();
        // pid 999999 with an explicit start time is not alive on this host -> died.
        let r = rec_json(serde_json::json!({
            "id":"x","state":"running","pid":999999,"host":"THIS-HOST",
            "start_time":"2020-01-01T00:00:00.0000000Z","started": now.to_rfc3339()
        }));
        let j = judgement(Some(&r), "", now, "THIS-HOST");
        assert_eq!((j.state.as_str(), j.exit), ("died", 1));
    }

    #[test]
    fn judgement_unreadable_on_problem() {
        let j = judgement(
            None,
            "the status file cannot be used: it is empty",
            Utc::now(),
            "HOST",
        );
        assert_eq!((j.state.as_str(), j.exit), ("unreadable", 1));
    }

    #[test]
    fn member_of_and_finished_counts() {
        let now = Utc::now();
        let r = rec_json(serde_json::json!({
            "id":"x","state":"running","pid":999999,"host":"OTHER",
            "started": now.to_rfc3339(),
            "members":[
                {"position":1,"state":"usable"},
                {"position":2,"state":"running"},
                {"position":3,"state":"skipped","outcome":"weighty reviewer"},
                {"position":4,"state":"skipped","outcome":"not started: the run ended"}
            ]
        }));
        let j = judgement(Some(&r), "", now, "THIS");
        // of = usable + running + the not-started skip = 3 (the weighty skip never ran).
        assert_eq!(j.of, 3);
        // finished = usable + the not-started skip = 2.
        assert_eq!(j.finished, 2);
    }

    #[test]
    fn format_span_thresholds() {
        assert_eq!(format_span(45.0), "45 s");
        assert_eq!(format_span(89.0), "89 s");
        assert_eq!(format_span(120.0), "2 min");
        assert_eq!(format_span(3.0 * 3600.0 + 5.0 * 60.0), "3 h 05 min");
        assert_eq!(format_span(3.0 * 86400.0 + 4.0 * 3600.0), "3 d 04 h");
    }

    #[test]
    fn list_line_blank_for_done() {
        let r = rec_json(serde_json::json!({"id":"x","state":"done","exit":0}));
        assert_eq!(
            list_line(Some(&r), "", "abcd1234", "t", Utc::now(), "HOST"),
            ""
        );
        let now = Utc::now();
        let r =
            rec_json(serde_json::json!({"id":"x","state":"starting","started":now.to_rfc3339()}));
        let l = list_line(Some(&r), "", "abcd1234", "my-task", now, "HOST");
        assert!(l.starts_with("detached abcd1234: starting"));
        assert!(l.contains("-Status -Id abcd1234"));
    }

    #[test]
    fn budget_takes_the_larger_of_group_and_cap_plus_slack() {
        // one group of 3 members, limit 1, each guard 100 -> waves 3 * 100 = 300.
        let guard = |_p: i64| 100i64;
        let groups = vec![(1i64, vec![1, 2, 3])];
        assert_eq!(detached_budget(&groups, &guard, 0, 120), 300 + 120);
        // with a cap of 2: ceil(3/2)=2 waves * longest(100)=200; group=300 dominates.
        assert_eq!(detached_budget(&groups, &guard, 2, 120), 300 + 120);
        // a single run: one group of one, guard 100 -> 100 + 120.
        assert_eq!(detached_budget(&[(1, vec![1])], &guard, 0, 120), 220);
    }

    #[test]
    fn member_guard_adds_repair_and_continuation() {
        assert_eq!(member_guard(1800, 0, false, false), 1800 + 180);
        assert_eq!(
            member_guard(1800, 900, true, true),
            1800 + 180 + 300 + 300 + 900
        );
        // the repair/denial cap is min(timeout, 300).
        assert_eq!(member_guard(100, 0, true, true), 100 + 180 + 100 + 100);
    }

    #[test]
    fn read_status_rejects_bad_records() {
        let dir =
            std::env::temp_dir().join(format!("c3-detach-test-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("bad.status.json");
        std::fs::write(&p, "{}").unwrap();
        assert!(read_detached_status(&p)
            .unwrap_err()
            .contains("names no id"));
        std::fs::write(&p, "{\"id\":\"x\",\"state\":\"weird\"}").unwrap();
        assert!(read_detached_status(&p)
            .unwrap_err()
            .contains("not one of starting|running|done"));
        std::fs::write(&p, "not json").unwrap();
        assert!(read_detached_status(&p)
            .unwrap_err()
            .contains("does not parse"));
        // a good one parses.
        std::fs::write(&p, "{\"id\":\"x\",\"state\":\"running\"}").unwrap();
        assert!(read_detached_status(&p).unwrap().is_some());
        // an absent file is Ok(None).
        assert!(read_detached_status(&dir.join("absent.json"))
            .unwrap()
            .is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
