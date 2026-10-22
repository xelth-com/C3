# Handoff 11 - Meta Muse (muse): reply-meta

Date: 2026-10-22 09:30 local. Author: Meta Muse (muse) (model muse-spark-1.3-contributor, effort high), muse-cli 1.4.11-R6667.1.
Reviewer: meta :: muse-spark-1.3-contributor [muse] (provider from roster, model from roster; engine muse (C:\Users\Dmytro\AppData\Local\Programs\muse\muse.cmd); provider fingerprint 1c6f62bb040d; harness muse-cli 1.4.11-R6667.1).
Preflight: ok: signed in (usable reply 1 min ago).
Effort: high sent (requested high, mapping muse-v1, by caps-v1: engine:muse, muse-spark-1.3-contributor; not confirmed by the provider). Consultation id: 42509b30-044a-454d-b132-887e429279ad.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `muse exec --json --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-139c7706bfea4877afbd6b2d31f74ad0.txt --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json --model muse-spark-1.3-contributor --reasoning-effort high --no-foreign-personal-context --disable-web-tools --disable-write --disable-shell --approval-mode never` (prompt from a file: --prompt-file).
Parent thread: (none - new thread; no thread of lineage meta :: muse-spark-1.3-contributor in this task's ledger; other lineage(s): openai :: gpt-6.1-sol, meta :: muse-wonder-1.0 [muse]; 1 unverified rollout candidate(s) are never parents). Result thread: `01a16283-9bcb-7a61-ac52-415d9d57f5e0` (source: events).
Brief: `.collab/consilium-2026-10-22/handoffs/01-claude-remedy-proposals.md` (sha256 e707d1d341d5). Reviewed: 23d49ad, base 23d49adbb5ae6f0cd3cc2c651eba44352ef5869c, tree sha256 673507ea4b1f, 0 changed files.
Bridge outcome: usable reply. Wall time: 85.3 s. Tokens: not reported by muse.
Engine turns: 1 (each one a Muse Code subscription prompt); MSP schema_version 1.
Warnings: muse: warning: rules file at \\?\C:\Users\Dmytro\AppData\Local\Temp\c3-consilium-2026-10-22\AGENTS.md exists, but the workspace is untrusted, so it is skipped for this session; restart with --trust-workspace to load project rules.
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - Remedy competition is worth a narrow spike on the two blockers, not a full panel and router change yet. Findings: 2 blocker, 3 major, 1 minor, 0 note (F11-1..F11-6, tracked in `findings.json`). Structured reply: `handoffs/11-muse-reply-meta.reply.json`.
Raw event stream: `handoffs/11-muse-reply-meta.events.jsonl`.
Verbatim reply follows.

---

## Framing challenge

The brief assumes the loop (fix closes simple case, next pass finds remaining case) is caused by finder taste plus worker discretion, so the fix is more solvers from other labs. Unlisted options: (a) keep one solver but fix the contract — structured `remedy` + finder acceptance criteria + coordinator-built worker brief; (b) single-solver spike, not panel of 3; (c) rule-fix with no panel (fail-closed admission, strict terminal detector); (d) pause `c3 work` acceptance until the admission invariant is proven. Vendor diversity does not guarantee case coverage — three solvers can share the same blind spot if the pack hides TOCTOU and permission paths.

To choose I need: remaining-case rate across passes, window cost of 3 solvers vs 40–60 min worker wave, whether packs reproduce races/denials, and whether router has any solver signal today.

## Q1 — Separation and verifier

Separation is sound for design breadth, but three consultations per blocker is too expensive as default. Verifier should be: solver-shipped test (failing then passing) re-run by the coordinator, plus finder re-inspection on the next pass. A third member only on disagreement. Failure modes: solver without codebase invents ungrounded patch (mitigate with pack + failing fixture); finder rejects foreign fixes (require machine-checkable criteria, coordinator decides); cost blowup (one solver first, expand only on disagreement).

## Q2 — Minimal design

Record: `findings[].proposals[]` {proposer, approach, locations, patch sketch, test, risks, confidence} + coordinator mark `adopted/rejected` with reason. Workflow: `c3 consult --remedy F-id --panel-size N --exclude-vendor <finder lab>` with role `solver-for-F-id`, pack = located code + existing tests + denial/race fixtures. Fits ledger: proposals under finding id, adoption feeds `prior_findings`. Must NOT automate: auto-run on HOLD, auto-pick by confidence, auto-implement, or router writes before verification.

## Q3 — Remedies (I am not the finder, so I propose)

**F91-1 (blocker, stale resume):** in `claim_for_spawn` / `resume_once` (`work/mod.rs`), move status, exhaustion, owner, and generation checks inside `admission_lock`; compute `n` from the locked record; reject stale callers before any launch marker. Invariant: one running attempt, dense monotonic numbers. Test: `work_resume_stale_claim_refused` with barrier before second claim; first goes `done` and separately `exhausted`; delayed claim must refuse.

**F91-2 (blocker, fail-open reads):** in `read_record`, `run_dirs`, `live_run`, `other_live_run` (`work/record.rs`), return inspection errors; only `NotFound` is absence; any denial/parse/enumeration refuses admission. Invariant: unknown means refuse. Test: `work_admission_denied_read_refuses` with live worker + denied `run.json`. F91-3/F91-4 deferred to keep this spike to two blockers; same pattern applies (propagate read errors; require explicit terminal evidence).

## Q4 — Scoring

Solver point only on verified close (adopted + RC passes + finder closes next pass); partial if merged. Finder point only if blocker survives to next pass or is rated useful; decay if overturned. Keep `finder` and `solver` as separate router keys; require ≥2 verified closes before routing weight, so easy blockers cannot be farmed.

## Requested checks

- RC1: repo root, workspace-write, barrier + denied-read fixtures for F91-1/F91-2; expect refusal before launch marker. Budget: 15 min.
- RC2: repo root, workspace-write, exit-zero agent_message-only / result-without-is_error streams; expect `died` never `done`. Budget: 10 min.

---

### Findings

- **F11-1** [major] `.collab/consilium-2026-10-22/handoffs/01-claude-remedy-proposals.md` - Do not build the full remedy panel yet; run a single-solver spike on the two F91 blockers first because the brief's framing assumes the miss is finder taste rather than an underspecified remedy contract. Trigger: Coordinator accepts the vendor-diversity panel as the only fix for the fix-missed-case loop. Evidence: read-code: Pass 2 text says 2 of 4 findings are 'remaining case' of pass-1 blockers and remedy is one sentence direction, worker makes design choices alone.; read-code: F91-1 stale-claim, F91-2 fail-open on unreadable records, F91-3 destructive read-default, F91-4 weak terminal detector all cite locked reread / default-on-error paths. Verify: Compare next-pass remaining-case rate with tightened remedy text versus full 3-solver panel on the same two blockers. Remedy: Before any new purpose or router dimension, require structured remedy plus finder acceptance criteria in the existing schema, run one solver on F91-1/F91-2, and only generalize if the remaining-case rate drops. Alternatives the brief omits: richer remedy field alone, coordinator-generated worker brief from finding, single solver not panel of 3, rule-fix without panel, or pausing c3 work acceptance.
- **F11-2** [major] `.collab/consilium-2026-10-22/handoffs/01-claude-remedy-proposals.md` - Finder/solver split is sound but verifier should be the solver-shipped failing test re-run by the coordinator plus finder re-inspection, not a third member by default. Trigger: A blocker enters remedy; solver lacks repo access or finder rejects every foreign fix. Evidence: read-code: Sketch assigns verifier as finder or third member; finder already costs ~280s and 9-16% window per pass with 40-60min worker wave between.; inferred: Three consultations per blocker triples window cost and adds finder-rejects-foreign-fix and solver-without-codebase failure modes. Verify: RC1-RC3 in handoff 91 pass without extra consultation: barrier resume refused, denied-read refused, bad terminal streams yield died not done. Remedy: Finder states machine-checkable acceptance criteria; solver must ship patch sketch plus failing-then-passing test; coordinator runs test, finder confirms trigger closed on next pass. Use a third verifier only on solver-finder disagreement or when finder history shows high reject rate.
- **F11-3** [major] `crates/c3/src/consult` - Minimal remedy record is findings[].proposals[] with proposer identity, approach, code locations, patch sketch, test, risks, confidence, plus coordinator decision adopted/rejected with reason; fit it in files-are-the-record without new automation. Trigger: HOLD with blockers where coordinator wants competing fixes under ledger. Evidence: read-code: Bridge already has findings[].remedy, prior_findings per-id status, roles by score rank with Kuhn matching, pack builder, and c3 work worker.; inferred: Pack of located code plus existing tests is lossy for TOCTOU and permission paths unless denial fixtures are included. Verify: Inspect findings.json: proposals stored under finding id with identity, exactly one adopted with reason, no auto-run on HOLD. Remedy: Add purpose remedy reusing consult reply schema, roles solver-for-F-id, pack must include located code plus failing fixtures and record/attempt fixtures. Coordinator records adopted/rejected with reason in findings.json. Must NOT automate: auto-trigger on HOLD, auto-pick by confidence, auto-implement without coordinator choice, or solver-score writes without verified fix.
- **F11-4** [blocker] `crates/c3/src/work/mod.rs:747`, `crates/c3/src/work/mod.rs:1142`, `crates/c3/src/work/mod.rs:1386` - F91-1 remedy: move all admission validation inside the admission lock in claim_for_spawn/resume_once and derive the attempt number from the same locked record with generation check. Trigger: Two resumes race; second resumes after first reached done or exhausted=true with stale n. Evidence: read-code: F91-1: status/exhaustion checked before lock, attempt n calculated before lock, locked reread checks only live owner then sets running unconditionally.; read-code: Module owns guards and run via record/detach lifecycle; record written by c3 never worker. Verify: Test work_resume_stale_claim_refused: barrier before second claim_for_spawn, first reaches done and separately exhausted, require delayed claim refuses with no launch marker. Remedy: Under admission_lock, reread RunRecord, reject if terminal or exhausted or owner mismatch or caller attempt != locked attempts.len()+1, compute n from locked record, then mark running atomically. Stale caller gets refusal before any child-launch marker. Invariant: at most one running attempt per task and attempt numbers are dense and monotonic.
- **F11-5** [blocker] `crates/c3/src/work/record.rs:137`, `crates/c3/src/work/record.rs:187`, `crates/c3/src/work/record.rs:211`, `crates/c3/src/work/record.rs:221` - F91-2 remedy: make admission fail-closed on inspection errors through read_record, run_dirs, live_run, and other_live_run. Trigger: Live worker holds run.json that a second starter cannot read, or enumeration/entry error occurs. Evidence: read-code: F91-2: every fs::read error returns Ok(None) including access failures; run_dirs empty on enum failure, flattens entry errors, live_run and other_live_run discard read/parse errors.; assumed: Exact error-enum signatures in record.rs not re-read under 6-call budget; shape follows F91-2 locations. Verify: Test work_admission_denied_read_refuses: live blocking worker plus read-deny on its run.json, require second start/resume refuses before launch. Remedy: Return Result with typed InspectionError; treat only confirmed NotFound as absence; any denied/parse/enumeration error refuses admission before launch with a distinct message. Invariant: admission proceeds only from fully inspected state; unknown means refuse. Add read-deny fixture using OS ACL or injected PermissionDenied.
- **F11-6** [minor] `crates/c3/src/router` - Credit solver only on verified fix and credit finder only on surviving blockers, with separate router dimensions and anti-gaming guards. Trigger: Adopted proposal merged or finder raises low-severity blockers to farm solver closes. Evidence: read-code: Sketch proposes adoption marks feed router as second solver dimension alongside finder ratings useful yes/partly/no keyed by consult_id.; inferred: Adoption alone rewards persuasive sketches; finder can inflate score by raising easy blockers that solvers fix trivially. Verify: Router query shows solver and finder dimensions diverge for a member with one lucky adopt versus repeated verified closes. Remedy: Solver gets solver-point only when its adopted proposal passes RC test and finder closes finding on next pass; partial credit if merged. Finder gets finder-point only if blocker survives worker fix to next pass or is rated useful, decayed if overturned as non-blocker. Keep finder and solver scores as separate router keys; require at least two verified closes before routing weight.

### Prior findings

_(none)_

## Verdict: ADVISE

Remedy competition is worth a narrow spike on the two blockers, not a full panel and router change yet.

### Blockers

- **F11-4** `crates/c3/src/work/mod.rs:747`, `crates/c3/src/work/mod.rs:1142`, `crates/c3/src/work/mod.rs:1386` - F91-1 remedy: move all admission validation inside the admission lock in claim_for_spawn/resume_once and derive the attempt number from the same locked record with generation check. Verify: Test work_resume_stale_claim_refused: barrier before second claim_for_spawn, first reaches done and separately exhausted, require delayed claim refuses with no launch marker. Remedy: Under admission_lock, reread RunRecord, reject if terminal or exhausted or owner mismatch or caller attempt != locked attempts.len()+1, compute n from locked record, then mark running atomically. Stale caller gets refusal before any child-launch marker. Invariant: at most one running attempt per task and attempt numbers are dense and monotonic.
- **F11-5** `crates/c3/src/work/record.rs:137`, `crates/c3/src/work/record.rs:187`, `crates/c3/src/work/record.rs:211`, `crates/c3/src/work/record.rs:221` - F91-2 remedy: make admission fail-closed on inspection errors through read_record, run_dirs, live_run, and other_live_run. Verify: Test work_admission_denied_read_refuses: live blocking worker plus read-deny on its run.json, require second start/resume refuses before launch. Remedy: Return Result with typed InspectionError; treat only confirmed NotFound as absence; any denied/parse/enumeration error refuses admission before launch with a distinct message. Invariant: admission proceeds only from fully inspected state; unknown means refuse. Add read-deny fixture using OS ACL or injected PermissionDenied.
- **F03-4** (prior, not-checked) `crates/c3/src/work/mod.rs:747` - Q3/F91-1 remedy: move attempt derivation and resumability revalidation inside claim_for_spawn under the admission lock. claim_for_spawn (mod.rs:747) currently only rejects a different live owner and unconditionally sets running; resume_once (mod.rs:1142) computes n before the lock; do_resume_foreground (mod.rs:1386) checks status/exhaustion on a pre-lock record.
- **F03-5** (prior, not-checked) `crates/c3/src/work/record.rs:137` - Q3/F91-2 remedy: make admission fail closed on uninspectable records. read_record (record.rs:137) maps every fs::read error to Ok(None), run_dirs (record.rs:187) returns [] on read_dir failure and flattens entry errors, and live_run/other_live_run (record.rs:211/223) discard read errors via .ok(), so a live worker whose run.json is unreadable is invisible to the one-live-worker guard.

### Unproven scenarios

- Pack fidelity for TOCTOU barriers and OS-level read denials was not inspected.
- Router scoring shape and whether adoption signal exists were not read.
- F91-3/F91-4 code paths and exact terminal-detector conditions were not re-read; remedies follow Handoff 91 text.

### First-run checklist (observable)

_(none)_
