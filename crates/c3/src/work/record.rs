//! The work run's record (design v1, `docs/work-mode.md`): the `<NN>-<provider>-<model>`
//! directory under `<collab>/<task>/work/`, its `run.json`, the git diff c3 computes around
//! the child, and the summary parsed from the child's event stream.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `run.json` of one work run. Written `running` before the child starts, made final
/// (`done` | `died` | `timeout` | `refused`) when it ends; `args` carries the background's
/// options wire of a detached run (taken by the background, never a secret).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunRecord {
    /// The record directory's name (`NN-provider-model`).
    pub id: String,
    pub task: String,
    pub seq: i64,
    pub provider: String,
    pub model: String,
    pub engine: String,
    /// The argv after the launcher (no secrets travel in argv).
    pub argv: Vec<String>,
    /// The host-marker NAMES scrubbed from the child (never a value).
    pub child_env_scrubbed: Vec<String>,
    pub started: String,
    #[serde(default)]
    pub ended: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i64>,
    /// `running` while live, else `done` | `died` | `timeout` | `refused`.
    pub status: String,
    #[serde(default)]
    pub turns: Option<i64>,
    /// Tool name -> how often the assistant called it.
    #[serde(default)]
    pub tool_calls: BTreeMap<String, i64>,
    #[serde(default)]
    pub files_changed: Vec<String>,
    /// The result event's usage/cost when present.
    #[serde(default)]
    pub usage: Option<Value>,
    /// The run's own timeout (the `--wait` default of a detached run).
    #[serde(default)]
    pub timeout_sec: i64,
    /// The background's pid (a detached run, while it lives).
    #[serde(default)]
    pub pid: Option<i64>,
    /// The background's options wire (a detached run's `running` record only).
    #[serde(default)]
    pub args: Option<String>,
}

impl RunRecord {
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec_pretty(self).map_err(|e| e.to_string())
    }
}

/// Write `run.json` (atomic replace).
pub fn write_record(dir: &Path, rec: &RunRecord) -> Result<(), String> {
    let bytes = rec.to_bytes()?;
    c3_core::store::write_text_atomic(&dir.join("run.json"), &bytes).map_err(|e| e.to_string())
}

/// Read one `run.json` (`Err` says why when it exists but does not parse).
pub fn read_record(dir: &Path) -> Result<Option<RunRecord>, String> {
    let p = dir.join("run.json");
    let bytes = match std::fs::read(&p) {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    let mut rec: RunRecord = serde_json::from_slice(&bytes)
        .map_err(|e| format!("{} does not parse: {e}", p.display()))?;
    // A `running` record whose background is gone (the pid is dead, or it never reported one
    // within the start grace) is a run that DIED: say so on disk, so `--status`, `--wait` and
    // the one-live-run guard never wait on a ghost.
    if rec.status == "running" && !is_live(&rec) {
        rec.status = "died".into();
        rec.ended = Some(chrono::Utc::now().to_rfc3339());
        rec.args = None;
        let _ = write_record(dir, &rec);
    }
    Ok(Some(rec))
}

/// How long a `running` record may go without a reported pid before it counts as dead
/// (the background reports its pid within its first second).
const START_GRACE_SEC: i64 = 120;

/// Whether a `running` record's background is alive: its reported pid exists, or no pid was
/// reported yet and the record is younger than [`START_GRACE_SEC`]. Anything else is dead.
pub fn is_live(rec: &RunRecord) -> bool {
    if rec.status != "running" {
        return false;
    }
    match rec.pid {
        Some(pid) if pid > 0 => crate::liveness::proc::pid_alive(pid as u32, ""),
        _ => match chrono::DateTime::parse_from_rfc3339(&rec.started) {
            Ok(t) => {
                (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_seconds() < START_GRACE_SEC
            }
            Err(_) => false,
        },
    }
}

/// Every work run directory of a task's `work/` root, by sequence (`[]` when none). A
/// directory that holds no `run.json` is not a run.
pub fn run_dirs(work_root: &Path) -> Vec<(i64, PathBuf)> {
    let mut out: Vec<(i64, PathBuf)> = Vec::new();
    let rd = match std::fs::read_dir(work_root) {
        Ok(rd) => rd,
        Err(_) => return out,
    };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let first = name.split('-').next().unwrap_or("");
        if first.is_empty() || !first.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let seq: i64 = first.parse().unwrap_or(0);
        if seq <= 0 || !entry.path().join("run.json").is_file() {
            continue;
        }
        out.push((seq, entry.path()));
    }
    out.sort_by_key(|(seq, _)| *seq);
    out
}

/// Whether a live work run exists for the task (a `running` record): its id, or `None`.
pub fn live_run(work_root: &Path) -> Option<String> {
    run_dirs(work_root).into_iter().find_map(|(_, dir)| {
        read_record(&dir)
            .ok()
            .flatten()
            .filter(|r| r.status == "running")
            .map(|r| r.id)
    })
}

/// A directory-name-safe form of a provider label or model id: everything outside
/// `[A-Za-z0-9._-]` becomes `-` (never empty).
fn dir_part(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    out.truncate(60);
    if out.is_empty() {
        out.push('x');
    }
    out
}

/// The next record directory of a task: `<work>/<NN>-<provider>-<model>` with NN the next
/// 2-digit sequence (never one that exists).
pub fn allocate(
    work_root: &Path,
    provider: &str,
    model: &str,
) -> Result<(String, i64, PathBuf), String> {
    let next = run_dirs(work_root)
        .iter()
        .map(|(seq, _)| *seq)
        .max()
        .unwrap_or(0)
        + 1;
    for seq in next..(next + 50) {
        let id = format!("{seq:02}-{}-{}", dir_part(provider), dir_part(model));
        let dir = work_root.join(&id);
        if !dir.exists() {
            std::fs::create_dir_all(&dir).map_err(|e| {
                format!(
                    "could not create the work record directory '{}' ({e})",
                    dir.display()
                )
            })?;
            return Ok((id, seq, dir));
        }
    }
    Err(format!(
        "no free work record directory under '{}' (50 tried); remove old work runs",
        work_root.display()
    ))
}

// ------------------------------------------------------------------ the git diff (c3 runs git, not the worker)

/// One read-only git command's stdout lines (empty on any failure).
fn git_lines(repo: &Path, args: &[&str]) -> Vec<String> {
    std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map(|o| {
            if o.status.success() {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(|l| l.to_string())
                    .collect()
            } else {
                Vec::new()
            }
        })
        .unwrap_or_default()
}

/// `git status --short` lines (the baseline before the run, the state after it).
pub fn status_short(repo: &Path) -> Vec<String> {
    git_lines(repo, &["status", "--short"])
}

/// `git diff --stat` lines after the run.
pub fn diff_stat(repo: &Path) -> Vec<String> {
    git_lines(repo, &["diff", "--stat"])
}

/// The path a `git status --short` line names (the `R  old -> new` form names the new path).
fn path_of_status_line(line: &str) -> String {
    let p = line.get(3..).unwrap_or("").trim();
    match p.split_once(" -> ") {
        Some((_, new)) => new.trim().to_string(),
        None => p.to_string(),
    }
}

/// The files the run changed: the paths `git status --short` names after the run that the
/// baseline before it did not name, in the after-order, deduplicated.
pub fn files_changed(before: &[String], after: &[String]) -> Vec<String> {
    let base: Vec<String> = before.iter().map(|l| path_of_status_line(l)).collect();
    let mut out: Vec<String> = Vec::new();
    for p in after.iter().map(|l| path_of_status_line(l)) {
        if !p.is_empty() && !base.contains(&p) && !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// `diff.txt`: the baseline, the after state and the diff stat.
pub fn diff_text(before: &[String], after: &[String], stat: &[String]) -> String {
    let block = |title: &str, lines: &[String]| -> String {
        if lines.is_empty() {
            format!("{title}: (empty)\n")
        } else {
            format!("{title}:\n{}\n", lines.join("\n"))
        }
    };
    format!(
        "{}\n{}\n{}",
        block("git status --short (before the run)", before),
        block("git status --short (after the run)", after),
        block("git diff --stat (after the run)", stat)
    )
}

// ------------------------------------------------------------------ the event summary

/// What c3 reads out of a work run's event stream (`events.jsonl`).
#[derive(Debug, Clone, Default)]
pub struct EventSummary {
    pub has_result: bool,
    /// The result event's `result` text (the report).
    pub result_text: String,
    pub is_error: bool,
    pub num_turns: Option<i64>,
    /// How many assistant events the stream holds (the `turns` fallback).
    pub assistant_count: i64,
    /// The last assistant text (the report of a run that died without a result).
    pub last_assistant_text: String,
    /// Tool name -> call count.
    pub tool_calls: BTreeMap<String, i64>,
    /// The result's `usage` merged with its `total_cost_usd` when present.
    pub usage: Option<Value>,
}

/// Parse the event stream into an [`EventSummary`] (tolerant: a line that is not a JSON
/// object is skipped).
pub fn summarize_events(text: &str) -> EventSummary {
    let mut s = EventSummary::default();
    for line in text.split('\n') {
        let t = line.trim();
        if t.is_empty() || !t.starts_with('{') {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<Value>(t) else {
            continue;
        };
        let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if ty == "assistant" {
            s.assistant_count += 1;
            let content = obj
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
            for b in content.iter().filter(|b| b.is_object()) {
                match b.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                    "text" => {
                        let tx = b.get("text").and_then(|v| v.as_str()).unwrap_or("");
                        if !tx.trim().is_empty() {
                            s.last_assistant_text = tx.to_string();
                        }
                    }
                    "tool_use" => {
                        let name = b
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("(unnamed)")
                            .to_string();
                        *s.tool_calls.entry(name).or_insert(0) += 1;
                    }
                    _ => {}
                }
            }
        } else if ty == "result" {
            s.has_result = true;
            s.result_text = obj
                .get("result")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            s.is_error = obj.get("is_error").and_then(|v| v.as_bool()) == Some(true);
            s.num_turns = obj.get("num_turns").and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_str().and_then(|x| x.parse().ok()))
            });
            let mut usage = obj.get("usage").filter(|v| v.is_object()).cloned();
            if let Some(cost) = obj.get("total_cost_usd").and_then(|v| {
                v.as_f64()
                    .or_else(|| v.as_str().and_then(|x| x.parse().ok()))
            }) {
                usage
                    .get_or_insert_with(|| Value::Object(Default::default()))
                    .as_object_mut()
                    .map(|o| o.insert("total_cost_usd".into(), Value::from(cost)));
            }
            s.usage = usage.filter(|v| v.is_object() && !v.as_object().unwrap().is_empty());
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let d = std::env::temp_dir().join(format!("c3-work-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_summary_counts_tools_and_reads_the_result() {
        let text = concat!(
            "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"u\",\"model\":\"glm-5.3\"}\n",
            "{\"type\":\"assistant\",\"message\":{\"model\":\"glm-5.3\",\"content\":[{\"type\":\"text\",\"text\":\"reading\"},{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"Read\",\"input\":{\"file_path\":\"a\"}},{\"type\":\"tool_use\",\"id\":\"t2\",\"name\":\"Bash\",\"input\":{}}]}}\n",
            "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"t1\"}]}}\n",
            "{\"type\":\"assistant\",\"message\":{\"model\":\"glm-5.3\",\"content\":[{\"type\":\"tool_use\",\"id\":\"t3\",\"name\":\"Write\",\"input\":{\"file_path\":\"b\"}},{\"type\":\"text\",\"text\":\"done\"}]}}\n",
            "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":4,\"result\":\"final report\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5},\"total_cost_usd\":0.02}\n"
        );
        let s = summarize_events(text);
        assert!(s.has_result && !s.is_error);
        assert_eq!(s.result_text, "final report");
        assert_eq!(s.last_assistant_text, "done");
        assert_eq!(s.num_turns, Some(4));
        assert_eq!(s.assistant_count, 2);
        assert_eq!(
            s.tool_calls.into_iter().collect::<Vec<_>>(),
            vec![
                ("Bash".to_string(), 1),
                ("Read".to_string(), 1),
                ("Write".to_string(), 1)
            ]
        );
        assert_eq!(s.usage.unwrap()["total_cost_usd"], 0.02);
        // a partial stream (a run that died) keeps the last assistant text
        let d = summarize_events(&text[..text.find("{\"type\":\"result\"").unwrap()]);
        assert!(!d.has_result);
        assert_eq!(d.last_assistant_text, "done");
        assert_eq!(d.num_turns, None);
    }

    #[test]
    fn files_changed_subtracts_the_baseline() {
        let before = vec!["?? .collab/".to_string(), " M src/lib.rs".to_string()];
        let after = vec![
            "?? .collab/".to_string(),
            " M src/lib.rs".to_string(),
            "?? done.txt".to_string(),
            "R  old.rs -> new.rs".to_string(),
        ];
        assert_eq!(files_changed(&before, &after), vec!["done.txt", "new.rs"]);
        assert_eq!(files_changed(&before, &before), Vec::<String>::new());
        assert_eq!(path_of_status_line("?? a.txt"), "a.txt");
        assert_eq!(path_of_status_line("R  old -> new"), "new");
        let d = diff_text(
            &before,
            &after,
            &[" src/lib.rs | 2 +-\n 1 file changed".to_string()],
        );
        assert!(d.contains("git status --short (before the run):"));
        assert!(d.contains("git status --short (after the run):"));
        assert!(d.contains("git diff --stat (after the run):"));
    }

    #[test]
    fn the_record_round_trips_and_allocates() {
        let work = scratch("record");
        let mut rec = RunRecord {
            id: "01-zai-claude-glm-5.3".into(),
            task: "t".into(),
            seq: 1,
            provider: "ZAI-claude".into(),
            model: "glm-5.3".into(),
            engine: "claude".into(),
            argv: vec!["-p".into(), "--model".into(), "glm-5.3".into()],
            child_env_scrubbed: vec!["CLAUDE_CODE_SOMETHING".into()],
            started: "2026-10-20T10:00:00+00:00".into(),
            status: "done".into(),
            timeout_sec: 3600,
            ..Default::default()
        };
        let dir = work.join(&rec.id);
        std::fs::create_dir_all(&dir).unwrap();
        write_record(&dir, &rec).unwrap();
        assert_eq!(read_record(&dir).unwrap().unwrap().id, rec.id);
        assert!(live_run(&work).is_none());
        // a running record with a live pid (this process) is live; with a dead one it is
        // rewritten as died and never counts as live
        rec.status = "running".into();
        rec.pid = Some(std::process::id() as i64);
        write_record(&dir, &rec).unwrap();
        assert_eq!(live_run(&work).as_deref(), Some("01-zai-claude-glm-5.3"));
        rec.pid = None;
        write_record(&dir, &rec).unwrap();
        assert!(
            live_run(&work).is_none(),
            "an old running record without a pid is dead"
        );
        assert_eq!(read_record(&dir).unwrap().unwrap().status, "died");
        rec.pid = Some(std::process::id() as i64);
        rec.status = "done".into();
        write_record(&dir, &rec).unwrap();
        assert!(live_run(&work).is_none());
        // the next sequence continues past it; the parts are sanitized
        let (id2, seq2, dir2) = allocate(&work, "byteplus-claude", "deepseek/v4.1 flash").unwrap();
        assert_eq!(seq2, 2);
        assert!(id2.starts_with("02-byteplus-claude-deepseek-v4.1-flash"));
        assert!(dir2.is_dir());
        let _ = std::fs::remove_dir_all(&work);
    }
}
