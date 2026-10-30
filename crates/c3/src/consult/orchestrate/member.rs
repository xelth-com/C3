//! The **panel member path** of a `c3 consult` run: a reviewer launched as one seat of a
//! panel rather than as a standalone run. [`run_member`] is the entry the dispatcher takes
//! for `--member`; it builds the member [`Options`] ([`member_options`]), takes the
//! early-accept shortcut when the reservation is already this process's
//! ([`member_early_accept`]), and otherwise dispatches a normal build/run. The acceptance
//! side ([`member_accept`], [`continuation_on_running`]) rewrites a running reservation as
//! this seat's and decides the continuation-kick warning ([`CONTINUATION_KICK_WARNING`]),
//! and [`build_member_panel`] / [`member_skips`] assemble the panel record for the ledger.

use super::*;

/// The panel member re-exec (`--panel-spec`): decode the spec, reconstruct the run's options
/// from `spec.args`, and run it as the panel run's member (never a single-run reservation).
pub(super) fn run_member(o: Options, home: Option<&str>) -> i32 {
    if o.panel || o.panel_all {
        return refuse("-PanelSpec is internal to -Panel; never combine them.");
    }
    let spec = match crate::panel::member::MemberSpec::from_wire(&o.panel_spec) {
        Ok(s) => s,
        Err(e) => {
            return refuse(&format!(
                "-PanelSpec is internal to -Panel and could not be read ({}).",
                c3_core::one_line(&e)
            ))
        }
    };
    if !spec.names_member() {
        return refuse(
            "-PanelSpec is internal to -Panel and does not name this member's numbers and parent (n, nn, parent_pid); this panel member was not started.",
        );
    }
    let mut mo = member_options(&o.task, &spec);
    let r = match args::validate(&mo, home) {
        Ok(r) => r,
        Err(msg) => return refuse(&msg),
    };
    r.apply_companions(&mut mo);
    // The member rewrites its reserved record as its own, waits the test-pause, and checks the
    // parent is alive — all BEFORE its preflight (`codex-consult.ps1:2074-2113`), so the
    // kill-after-rewrite window (F07-1/F11-6) is real and a parent that dies during the preflight
    // is caught by the pre-launch check (in `run_live`) with "stopped before starting", not here.
    // A dry-run member has no record.
    if !mo.dry_run {
        if let Err(msg) = member_early_accept(&mo, &spec) {
            return refuse(&msg);
        }
    }
    dispatch(build_context(mo, r, Some(&spec)))
}

/// The member's record rewrite + pause hook + parent-alive check, done before its preflight
/// (`codex-consult.ps1:2074-2113`). Rewrites the reserved record with this process's pid/host,
/// keeping every other field (state stays `reserved`), then — after the optional test pause —
/// refuses (and withdraws the record) if the parent is gone.
fn member_early_accept(mo: &Options, m: &crate::panel::member::MemberSpec) -> Result<(), String> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let collab_root = providers::resolve_collab_root(&repo_root, &mo.collab_dir);
    let task = TaskSlug::new(mo.task.clone()).map_err(|e| e.to_string())?;
    let store = FilesStore::new(collab_root);
    let pending = PendingRef::member(task, m.nn as u32);
    let path = store_pending_path(&store, &pending);
    let started = "This panel member was not started.";
    // (wave 7d, F42-1 sweep) only an absent record (or no file) "does not exist"
    match std::fs::metadata(&path) {
        Ok(m) if m.is_file() => {}
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!(
                "could not read the recovery record '{}' ({e}). {started}",
                path.display()
            ))
        }
        _ => {
            return Err(format!(
                "this panel member's recovery record '{}' does not exist (the panel run writes it before it launches a member); this panel member was not started.",
                path.display()
            ))
        }
    }
    let bytes = std::fs::read(&path).map_err(|e| {
        format!(
            "could not read the recovery record '{}' ({e}). {started}",
            path.display()
        )
    })?;
    let val: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
        format!(
            "the recovery record '{}' is unusable ({e}). {started}",
            path.display()
        )
    })?;

    let mut mism: Vec<String> = Vec::new();
    match val.get("panel") {
        None | Some(serde_json::Value::Null) => mism.push("the record names no panel".into()),
        Some(p) => {
            add_mismatch(&mut mism, "panel id", json_str(p, "id"), m.id.clone());
            add_mismatch(
                &mut mism,
                "parent pid",
                json_str(p, "parent_pid"),
                m.parent_pid.to_string(),
            );
            add_mismatch(
                &mut mism,
                "parent start time",
                json_str(p, "parent_start_time"),
                m.parent_start_time.clone(),
            );
        }
    }
    add_mismatch(&mut mism, "n", json_str(&val, "n"), m.n.to_string());
    add_mismatch(
        &mut mism,
        "nn",
        json_str(&val, "nn"),
        format!("{:02}", m.nn),
    );
    add_mismatch(
        &mut mism,
        "state",
        json_str(&val, "state"),
        "reserved".to_string(),
    );
    add_mismatch(
        &mut mism,
        "writer pid",
        json_str(&val, "pid"),
        m.parent_pid.to_string(),
    );
    if !mism.is_empty() {
        return Err(format!(
            "this panel member's recovery record '{}' does not match its spec ({}); this panel member was not started.",
            path.display(),
            mism.join("; ")
        ));
    }

    // (wave 7d, F42-1 sweep) a record that does not read back as one is unusable, never a default
    let mut rec: PendingRecord = serde_json::from_value(val).map_err(|e| {
        format!(
            "the recovery record '{}' is unusable ({e}). {started}",
            path.display()
        )
    })?;
    // The member rewrites the reserved record as its OWN (the writer-pid rule): its own pid when
    // the panel run launched it (the panel clears the bridge var for members), or the shim's pid
    // when a harness launched the member directly through the shim (SPEC).
    let (pid, start_time) = crate::liveness::proc::bridge_identity();
    rec.pid = pid;
    rec.start_time = start_time;
    rec.host = pending_host();
    rec.note = format!(
        "review panel member (the panel run is pid {})",
        m.parent_pid
    );
    if let Err(e) = store.write_pending(&pending, &rec) {
        return Err(format!(
            "could not rewrite this panel member's recovery record '{}': {e}; this panel member was not started.",
            path.display()
        ));
    }

    // TEST HOOK: CODEX_CONSULT_TEST_MEMBER_PAUSE_MS - a pause between the rewrite and the parent
    // check (the harness kills the parent inside it).
    if let Some(ms) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_MEMBER_PAUSE_MS") {
        if let Ok(ms) = ms.trim().parse::<u64>() {
            if ms > 0 {
                std::thread::sleep(Duration::from_millis(ms));
            }
        }
    }

    if !crate::liveness::proc::pid_alive(m.parent_pid as u32, &m.parent_start_time) {
        let withdrawn = std::fs::remove_file(&path).is_ok();
        return Err(format!(
            "the review panel run that launched this member (pid {}) is gone; this panel member was not started - nothing was started and its recovery record '{}' {}.",
            m.parent_pid,
            path.display(),
            if withdrawn {
                "was withdrawn".to_string()
            } else {
                "could not be withdrawn (the next run consumes it)".to_string()
            }
        ));
    }
    Ok(())
}

/// Reconstruct a run's [`Options`] from a panel member spec's `args` object (`$pa`,
/// `codex-consult.ps1:1749-1786`) plus the task from the command line. Provider/model/thread are
/// never in `args` (a member takes its identity from the spec's roster entry); the numbers,
/// consult id and role come from the spec's top-level fields, handled in `build_context`.
fn member_options(task: &str, spec: &crate::panel::member::MemberSpec) -> Options {
    let a = &spec.args;
    let s = |k: &str| a.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let i = |k: &str, d: i64| a.get(k).and_then(|v| v.as_i64()).unwrap_or(d);
    let b = |k: &str| a.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let list = |k: &str| {
        a.get(k)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let collab = s("collab_dir");
    let continue_sec = i("continue_sec", -1);
    Options {
        task: task.to_string(),
        // (M11) a panel passes `--peer`/`--peers` through to its http members only; the member
        // spec carries them and they are set below when present (parked default here).
        peer: list("peer"),
        peers: {
            let v = s("peers");
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        },
        collab_dir: if collab.is_empty() {
            ".collab".into()
        } else {
            collab
        },
        mode: s("mode"),
        thread: String::new(),
        brief: s("brief"),
        prompt: s("prompt"),
        model: String::new(),
        purpose: s("purpose"),
        effort: s("effort"),
        sandbox: s("sandbox"),
        max_words: i("max_words", 0),
        timeout_sec: i("timeout_sec", 0),
        continue_sec,
        continue_sec_given: continue_sec != -1,
        stall_sec: i("stall_sec", -1),
        stall_sec_given: i("stall_sec", -1) != -1,
        // (wave 8 = plugin wave 30) the quota budget the panel run resolved, and its source
        budget: a.get("budget").and_then(|v| v.as_i64()).filter(|b| *b >= 0),
        budget_source: s("budget_source"),
        range: s("range"),
        reply_name: s("reply_name"),
        artifacts: list("artifact"),
        raw: b("raw"),
        codex_exe: s("codex_exe"),
        provider: String::new(),
        // A panel http seat takes its base_url/key_env/pack_tokens from the roster entry (looked
        // up by the child from its own roster read), never from the member spec's args.
        key_env: String::new(),
        base_url: String::new(),
        pack_budget: -1,
        native_effort: s("native_effort"),
        off_peak_only: b("off_peak_only"),
        skip_preflight: b("skip_preflight"),
        codex_config: list("codex_config"),
        schema_transport: s("schema_transport"),
        // (0.6.1 parity) the panel run's own `--telemetry on|off`, else the environment decides.
        telemetry: match s("telemetry").as_str() {
            "on" => Some(true),
            "off" => Some(false),
            _ => None,
        },
        format_retry: i("format_retry", 1),
        dry_run: b("dry_run"),
        engine: s("engine"),
        engine_exe: s("engine_exe"),
        denial_retry: i("denial_retry", 1),
        max_model_steps: i("max_model_steps", 0),
        panel: false,
        panel_all: false,
        panel_concurrency: 0,
        panel_concurrency_given: false,
        panel_size: 0,
        panel_size_given: false,
        panel_order: String::new(),
        panel_seed: String::new(),
        require: Vec::new(),
        role: String::new(),
        roles: Vec::new(),
        topic: list("topics"),
        panel_spec: String::new(),
        detach: false,
        status: false,
        id: String::new(),
        id_given: false,
        detach_id: String::new(),
        list: false,
        wait: false,
        wait_timeout_sec: 0,
        wait_timeout_sec_given: false,
        prune: false,
        kick: false,
        member: String::new(),
        // (wave 27) the panel run's resolved brief prefix (shown by a member's dry run)
        brief_prefix: s("brief_prefix"),
    }
}

/// Record a member-record/spec mismatch in the plugin's phrasing (`$compare`, line 2083).
fn add_mismatch(mism: &mut Vec<String>, name: &str, in_record: String, in_spec: String) {
    if in_record != in_spec {
        let shown = if in_record.is_empty() {
            "(none)".to_string()
        } else {
            in_record
        };
        mism.push(format!(
            "{name} {shown} in the record, {in_spec} in the spec"
        ));
    }
}

/// A JSON value's field as its string form (a number renders without quotes, matching the
/// plugin's `[string]$value`).
fn json_str(v: &serde_json::Value, key: &str) -> String {
    match v.get(key) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// Phase B of the panel-member accept (`codex-consult.ps1:3516-3524`): the record was already
/// rewritten as this process's own and its parent verified alive by [`member_early_accept`]
/// (before the preflight); reopen it and stamp this run's reply/consult id/launcher/engine. A
/// missing record here means it was withdrawn under this process; the base record is used as-is.
/// (Wave 7d, F42-1 sweep) A record that cannot be read or parsed is no missing one: the member is
/// refused before its launch (`Err`), the record kept.
pub(super) fn member_accept(
    store: &FilesStore,
    ctx: &Context,
    pending: &PendingRef,
    _m: &crate::panel::member::MemberSpec,
    reply_rel: &str,
) -> Result<PendingRecord, String> {
    let path = store_pending_path(store, pending);
    let started = "This panel member was not started.";
    let mut rec: PendingRecord = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
            format!(
                "the recovery record '{}' is unusable ({e}). {started}",
                path.display()
            )
        })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => PendingRecord::default(),
        Err(e) => {
            return Err(format!(
                "could not read the recovery record '{}' ({e}). {started}",
                path.display()
            ))
        }
    };
    rec.reply = reply_rel.to_string();
    rec.consult_id = ctx.consult_id.clone();
    rec.launcher = if ctx.is_codex() {
        ctx.launcher.clone()
    } else {
        ctx.engine_launcher.clone()
    };
    rec.engine = ctx.engine.clone();
    let _ = store.write_pending(pending, &rec);
    Ok(rec)
}

/// (wave 27c, D2) The run warning recorded when the operator kicks the timeout CONTINUATION: the
/// continuation is cancelled but the run keeps the main turn's timeout outcome and its salvage.
pub(super) const CONTINUATION_KICK_WARNING: &str =
    "kick: the operator stopped the timeout continuation (-Kick); the timeout outcome and its salvage stay";

/// (wave 27c, D2) The `on_running` callback for the timeout continuation turn: it rewrites the
/// pending record to `running` naming the CONTINUATION's child pid/start the moment it spawns, so a
/// concurrent `--kick` (and the liveness check) find a live child of this run — the main turn's child
/// was killed at the timeout. Built here so it is unit-testable in isolation.
pub(super) fn continuation_on_running(
    store: &FilesStore,
    pending: &PendingRef,
    base: &PendingRecord,
) -> std::sync::Arc<dyn Fn(u32, String) + Send + Sync> {
    let cb_store = store.clone();
    let cb_pending = pending.clone();
    let cb_base = base.clone();
    std::sync::Arc::new(move |child_pid: u32, child_start: String| {
        let mut r = cb_base.clone();
        r.state = PendingState::Running;
        r.child_pid = Some(child_pid);
        r.child_start_time = child_start;
        r.note = "the timeout continuation turn is running".to_string();
        let _ = cb_store.write_pending(&cb_pending, &r);
    })
}

/// Build the salvaged `.partial.md` body, footer and resume command when a killed turn had no
/// usable continuation (`codex-consult.ps1:4108`).
/// The `entry.panel` object a panel member records (`codex-consult.ps1:3216`): the panel id, this
/// seat, the panel-wide member list (with each seat's state/reason — the context skip included),
/// the plan and the routing record. `started`/`usable` are the panel run's to patch after every
/// member finishes. `None` for a single run. Shared by the ledger entry and the dry-run preview.
pub(crate) fn build_member_panel(ctx: &Context) -> Option<c3_core::ledger::Panel> {
    let m = ctx.panel_member.as_ref()?;
    let members = m
        .members
        .iter()
        .map(|b| c3_core::ledger::PanelMember {
            provider: b.provider.clone(),
            model: b.model.clone(),
            state: b.state.clone(),
            reason: b.reason.clone(),
            ..Default::default()
        })
        .collect();
    let routing = m
        .routing
        .as_ref()
        .and_then(|v| serde_json::from_value::<c3_core::ledger::PanelRouting>(v.clone()).ok());
    Some(c3_core::ledger::Panel {
        id: m.id.clone(),
        position: m.position,
        of: m.of,
        members,
        concurrency: m.concurrency,
        limits: m.limits.clone(),
        asked: Some(if m.asked > 0 { m.asked } else { m.of }),
        started: Some(None),
        usable: Some(None),
        routing,
        roles_note: Some(Some(ctx.roles_note.clone())),
        ..Default::default()
    })
}

/// Parse a panel member spec's `skipped` value into the roster-skip tuples the ledger record
/// uses: each `{provider, model, engine?, reason}` in order.
pub(super) fn member_skips(skipped: &serde_json::Value) -> Vec<(String, String, String, String)> {
    let arr = match skipped.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter()
        .map(|v| {
            let g = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            (g("provider"), g("model"), g("engine"), g("reason"))
        })
        .collect()
}

#[cfg(test)]
mod continuation_kick_tests {
    use super::*;

    #[test]
    fn continuation_registers_its_child_in_the_pending_record() {
        // (D2) a `running` record naming the OLD (main-turn) child; the continuation's on_running
        // rewrites it to name the CONTINUATION's live child, so a concurrent `--kick` finds it.
        let dir = std::env::temp_dir().join(format!(
            "c3-contkick-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let task = TaskSlug::new("t").unwrap();
        std::fs::create_dir_all(dir.join("t")).unwrap();
        let store = FilesStore::new(dir.clone());
        let pending = PendingRef::single(task.clone());
        let base = PendingRecord {
            state: PendingState::Running,
            n: 1,
            nn: "01".into(),
            child_pid: Some(999_999), // the main turn's child, killed at the timeout
            child_start_time: "old".into(),
            ..Default::default()
        };
        let cb = continuation_on_running(&store, &pending, &base);
        let me = std::process::id();
        let start = crate::liveness::proc::process_start_iso(me).unwrap_or_default();
        cb(me, start.clone());

        let rd = crate::liveness::pending::read_pending_file(
            &store.task_dir(&task).join(".consult.pending.json"),
        );
        let rec = rd.record.expect("the record was rewritten");
        assert_eq!(
            rec.get("child_pid").and_then(|v| v.as_u64()),
            Some(me as u64),
            "the record names the continuation's child, not the first turn's"
        );
        assert_eq!(rec.get("state").and_then(|v| v.as_str()), Some("running"));
        // A concurrent `--kick` liveness check would now find a live child of this run.
        assert!(crate::liveness::proc::pid_alive(me, &start));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn continuation_kick_keeps_the_timeout_outcome_and_names_the_turn() {
        // (D2) the operator kicking the continuation records the operator stop on the continuation
        // turn, keeps the run's timeout outcome, and warns naming the cancelled turn.
        assert_eq!(
            CONTINUATION_KICK_WARNING,
            "kick: the operator stopped the timeout continuation (-Kick); the timeout outcome and its salvage stay"
        );
        // The `sec.continue_kicked` branch records the operator stop as the continuation's outcome
        // without touching the run's bridge_outcome (verified end-to-end by fixes27c KICK D2).
        let mut sec = Secondary {
            continue_kicked: true,
            ..Default::default()
        };
        assert!(sec.continue_kicked);
        sec.timeout_continue = Some(c3_core::ledger::TimeoutContinue {
            outcome: "failed: stopped by the operator (-Kick)".into(),
            ..Default::default()
        });
        assert!(sec
            .timeout_continue
            .as_ref()
            .unwrap()
            .outcome
            .contains("stopped by the operator (-Kick)"));
    }
}

#[cfg(test)]
mod member_tests {
    use super::*;
    use crate::panel::member::{MemberBrief, MemberSpec};
    use serde_json::json;

    fn scratch() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "c3-member-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(d.join("t")).unwrap();
        d
    }

    fn sample_spec(collab: &str, listed: Vec<String>, role: &str) -> MemberSpec {
        MemberSpec {
            id: "043d5bfe-1111-2222-3333-444455556666".into(),
            position: 2,
            of: 3,
            members: vec![MemberBrief {
                provider: "openai".into(),
                model: "gpt-6-astra".into(),
                state: "run".into(),
                reason: String::new(),
            }],
            roster_position: 1,
            provider: "openai".into(),
            model: "gpt-6-astra".into(),
            engine: String::new(),
            skipped: json!([]),
            listed_ids: listed,
            n: 12,
            nn: 7,
            consult_id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into(),
            parent_pid: 4321,
            parent_start_time: "2026-09-27T10:11:12.3456789Z".into(),
            sibling_nns: vec![6, 8],
            concurrency: 3,
            limits: json!({"openai": 1}),
            asked: 3,
            routing: Some(json!({"mode": "roster", "size": 3})),
            role: role.into(),
            roles_note: String::new(),
            panel_warnings: vec!["a framing panel seated below 2".into()],
            args: json!({
                "collab_dir": collab,
                "purpose": "framing",
                "prompt": "look at this",
                "reply_name": "reply-openai",
                "timeout_sec": 0,
                "continue_sec": -1,
                "format_retry": 1,
                "skip_preflight": true,
                "raw": false,
                "dry_run": false
            }),
        }
    }

    #[test]
    fn build_context_honours_the_member_spec() {
        let _g = super::telemetry_tests::ENV_LOCK.lock().unwrap();
        let root = scratch();
        let roster_path = root.join("roster.json");
        std::fs::write(
            &roster_path,
            r#"{"roster_version":1,"reviewers":[{"provider":"openai","model":"gpt-6-astra"}]}"#,
        )
        .unwrap();
        // Two open findings; the spec lists only the first.
        std::fs::write(
            root.join("t").join("findings.json"),
            r#"{"task_id":"t","findings":[{"id":"F01-1","status":"proposed","claim":"a"},{"id":"F02-1","status":"proposed","claim":"b"}]}"#,
        )
        .unwrap();
        // A repository role the member's role paragraph resolves.
        std::fs::create_dir_all(root.join(".collab").join("roles")).ok();
        std::fs::create_dir_all(root.join("t").join("roles")).ok();
        // The collab dir is `<root>` and the task `t`, so the roles dir is `<root>/roles`.
        std::fs::create_dir_all(root.join("roles")).unwrap();
        std::fs::write(root.join("roles").join("adversary.md"), "Attack it.").unwrap();

        std::env::set_var("CODEX_CONSULT_ROSTER", &roster_path);
        std::env::set_var("CODEX_CONSULT_TELEMETRY", "off");

        let spec = sample_spec(root.to_str().unwrap(), vec!["F01-1".into()], "adversary");
        let mo = member_options("t", &spec);
        let r = args::validate(&mo, None).expect("member options validate");
        let ctx = build_context(mo, r, Some(&spec)).expect("member build_context");

        // Numbers, consult id and reply file name come from the spec.
        assert_eq!(ctx.nn, 7);
        assert_eq!(ctx.consult_n, 12);
        assert_eq!(ctx.consult_id, "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee");
        assert!(
            ctx.reply_path
                .to_string_lossy()
                .ends_with("07-codex-reply-openai.md"),
            "{}",
            ctx.reply_path.display()
        );
        // The open-findings snapshot is intersected with `listed_ids`.
        assert_eq!(ctx.open_findings_count, 1);
        // The role paragraph is in the prompt, after the ask and before nothing else brief-side.
        assert!(ctx
            .prompt_text
            .contains("Your role in this review: adversary."));
        assert!(ctx.prompt_text.contains("Attack it."));
        // The panel warning is carried into the run warnings and the ledger.
        assert!(ctx
            .run_warnings
            .iter()
            .any(|w| w == "a framing panel seated below 2"));
        // The pending record is the member's, not the single-run file.
        assert_eq!(ctx.role, "adversary");
        assert!(ctx.panel_member.is_some());

        std::env::remove_var("CODEX_CONSULT_ROSTER");
        std::env::remove_var("CODEX_CONSULT_TELEMETRY");
        let _ = std::fs::remove_dir_all(root);
    }

    // (muse PANEL) a panel passes -MaxModelSteps to every member; a codex member (no step cap)
    // drops it instead of refusing (`if ($panelMember) { $MaxModelSteps = 0 }`), a single codex run
    // still refuses it.
    #[test]
    fn a_codex_member_drops_the_step_cap() {
        let _g = super::telemetry_tests::ENV_LOCK.lock().unwrap();
        let root = scratch();
        let roster_path = root.join("roster.json");
        std::fs::write(
            &roster_path,
            r#"{"roster_version":1,"reviewers":[{"provider":"openai","model":"gpt-6-astra"}]}"#,
        )
        .unwrap();
        std::env::set_var("CODEX_CONSULT_ROSTER", &roster_path);
        std::env::set_var("CODEX_CONSULT_TELEMETRY", "off");
        let mut spec = sample_spec(root.to_str().unwrap(), vec![], "");
        spec.args["max_model_steps"] = json!(40);
        let mo = member_options("t", &spec);
        assert_eq!(mo.max_model_steps, 40);
        let r = args::validate(&mo, None).unwrap();
        let ctx = build_context(mo, r, Some(&spec)).expect("a codex member runs");
        assert_eq!(ctx.o.max_model_steps, 0);
        let mut single = member_options("t", &spec);
        single.collab_dir = root.to_str().unwrap().to_string();
        let r2 = args::validate(&single, None).unwrap();
        let err = match build_context(single, r2, None) {
            Ok(_) => panic!("a single codex run refuses -MaxModelSteps"),
            Err(e) => e,
        };
        assert!(
            err.0.starts_with("-MaxModelSteps is for an engine with a model-step cap (muse --max-model-steps, claude --max-turns); the codex engine has none."),
            "{}",
            err.0
        );
        std::env::remove_var("CODEX_CONSULT_ROSTER");
        std::env::remove_var("CODEX_CONSULT_TELEMETRY");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn build_context_refuses_a_roster_that_changed_under_the_panel() {
        let _g = super::telemetry_tests::ENV_LOCK.lock().unwrap();
        let root = scratch();
        let roster_path = root.join("roster.json");
        // The roster entry at position 1 is a DIFFERENT reviewer than the spec names.
        std::fs::write(
            &roster_path,
            r#"{"roster_version":1,"reviewers":[{"provider":"zai","model":"glm-5.3"}]}"#,
        )
        .unwrap();
        std::env::set_var("CODEX_CONSULT_ROSTER", &roster_path);
        std::env::set_var("CODEX_CONSULT_TELEMETRY", "off");

        let spec = sample_spec(root.to_str().unwrap(), vec![], "");
        let mo = member_options("t", &spec);
        let r = args::validate(&mo, None).unwrap();
        let err = match build_context(mo, r, Some(&spec)) {
            Ok(_) => panic!("expected a roster-changed refusal"),
            Err(e) => e,
        };
        assert!(err.0.contains("changed while the panel ran"), "{}", err.0);

        std::env::remove_var("CODEX_CONSULT_ROSTER");
        std::env::remove_var("CODEX_CONSULT_TELEMETRY");
        let _ = std::fs::remove_dir_all(root);
    }
}
