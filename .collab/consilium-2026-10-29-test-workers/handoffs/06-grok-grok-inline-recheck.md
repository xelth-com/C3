# Handoff 06 - Grok (grok): grok-inline-recheck

Date: 2026-10-29 20:55 local. Author: Grok (grok) (model grok-4.7, effort high), grok-cli 1.0.70.
Reviewer: grok :: grok-4.7 [grok] (provider from -Provider, model from roster; engine grok (C:\Users\Dmytro\.grok\bin\grok.exe); provider fingerprint 8c4f8ce6de16; harness grok-cli 1.0.70).
Preflight: ok: signed in (~/.grok/auth.json) - subscription (grok.com oauth).
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - entry 14 of 14 for -Provider grok (engine, model applied).
Effort: high sent (requested high, mapping grok-v1, by caps-v1: engine:grok, any model; not confirmed by the provider). Consultation id: 80a6c870-4dcd-43f2-a73d-72b209d0c8f3.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `grok --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-bb07ba8409bd4a7083f05b5989d74466.txt --output-format streaming-json --json-schema {"$schema":"http://json-schema.org/draft-07/schema#","title":"codex-consult reply v1","type":"object","additionalProperties":false,"required":["schema_version","verdict","verdict_reason","reply_markdown","findings","prior_findings","unproven","first_run_checklist"],"properties":{"schema_version":{"type":"string","enum":["1"]},"verdict":{"type":"string","enum":["ACCEPT","HOLD","REJECT","ADVISE"]},"verdict_reason":{"type":"string"},"reply_markdown":{"type":"string"},"findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["severity","locations","claim","trigger","evidence","verification","remedy","supersedes"],"properties":{"severity":{"type":"string","enum":["blocker","major","minor","note"]},"locations":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["path","line"],"properties":{"path":{"type":"string"},"line":{"type":["integer","null"]}}}},"claim":{"type":"string"},"trigger":{"type":"string"},"evidence":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["kind","reference","observation"],"properties":{"kind":{"type":"string","enum":["read-code","ran-command","inferred","assumed"]},"reference":{"type":"string"},"observation":{"type":"string"}}}},"verification":{"type":"string"},"remedy":{"type":"string"},"supersedes":{"type":"array","items":{"type":"string"}}}}},"prior_findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["id","status","note"],"properties":{"id":{"type":"string"},"status":{"type":"string","enum":["fixed","still-open","not-checked","unknown-id"]},"note":{"type":"string"}}}},"unproven":{"type":"array","items":{"type":"string"}},"first_run_checklist":{"type":"array","items":{"type":"string"}}}} -m grok-4.7 --reasoning-effort high --disable-web-search --tools read_file,list_dir,grep --disallowed-tools use_tool,search_tool,write,run_terminal_command --permission-mode dontAsk --no-subagents` (prompt from a file: --prompt-file).
Parent thread: (none - new thread). Result thread: `01a18938-ef0e-7230-9f6e-f2a0480dc069` (source: events).
Brief: `.collab/consilium-2026-10-29-test-workers/handoffs/01-claude-cross-model-tests.md` (sha256 57c053d8e8cc). Reviewed: 0b712c4 + uncommitted, base 0b712c46cf717bdb90de5e0228b720d17388cfb9, tree sha256 b82f3b896236, 3 changed files.
Bridge outcome: usable reply. Wall time: 132.8 s. Tokens: in 13674 (cached 1664), out 8830, reasoning 2821.
Engine turns: 1 (each one a SuperGrok subscription prompt).
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - Do not make cross-model testing the default c3 pipeline until a patch is stored, author resume exists, and the gate can tell a red test from a compile failure. Findings: 1 blocker, 7 major, 0 minor, 1 note (F06-1..F06-9, tracked in `findings.json`). Structured reply: `handoffs/06-grok-grok-inline-recheck.reply.json`.
Raw event stream: `handoffs/06-grok-grok-inline-recheck.events.jsonl`.
Verbatim reply follows.

---

## Framing

The brief treats linkage, independence, and a validity oracle as one pipeline. They are three products. Linkage is a record. Independence is who may edit what. The oracle is a c3-owned check that tests fail before the change and pass after. Cross-model authorship does not create the oracle, and the oracle does not require a second model.

Options missing from the list:

- **Author-only tests plus a c3 gate.** The GLM run already wrote four integration tests and fixed a guard bug. Measure whether those tests fail on the pre-change tree before paying a second model.
- **Non-mutating tester.** The tester emits a patch; c3 applies it under scope and runs the gate. Today no engine has a path allowlist, so this is the only containment that can be real.
- **Frozen contract, not the author brief.** Blind testing against the same `brief.md` shares one misread. Independence needs a spec neither side may edit during the run.
- **Panel review of the diff** (ruled out of scope). That is the existing second-family look. A tester who edits the tree is a second author, which cuts against “one task = one author”.

“Default for every worker task” is the costly assumption. One live run per task, no resume of `done`, no full diff in the record, dirty baselines, and `-j 2` with no parallel cargo already make alternatives 2–4 unimplementable. Convention-only is the only option that runs today, and it has no audit trail.

What would decide it: ten paired tasks, scored by bugs the author tests missed and by false reds (compile-fail, flake, OS error 5), plus wall time under the OOM rule. One GLM run is not that sample.

## Q1

Build only a thin v1, and not as the default. Worth building: a work-set id, roles, a stored patch, a post-hoc scope check, and a coordinator verdict field. Not worth building yet: family refusal, launch-time path guards, or a gate on every task.

Alternative 1 stays correct until those records exist. Evidence that would push to a default: paired runs beating author-written tests on real bugs often enough to pay two model runs and two serialized `cargo test -j 2` cycles. Evidence that would kill it: most failures are spec disputes, compile-fails, or flakes.

Ship behind an opt-in. The resume gap (F02-2) and the one-live-run guard (F04-1) block the coordinator’s own loop.

## Q2

They catch different bugs.

- **Blind / spec-first** catches extra behavior, missed cases, and author tests that encode the bug. It fails when the spec is vague, when both sides share `brief.md`, and when the tester “fixes” the product so the tests pass. It cannot see an implementation bug the spec does not name.
- **Sighted** catches boundary mistakes and regressions the diff introduces. It fails by copying the implementation, including its bugs, and by treating the diff as instructions (F04-5, worse on unrestricted Muse, F02-7).

Use blind when a contract can be frozen before code. Use sighted for bugs and refactors, and feed a **patch**, not `diff.txt`. Today `diff.txt` is status plus `--stat` only (F02-1), so a sighted tester cannot see the change. Neither mode is an oracle until the gate separates assertion failures from compile failures (F03-1).

## Q3

The coordinator decides. c3 must not.

Record, immutable per attempt: work-set id, author and tester run ids, family/provider/model actually invoked, spec hash, patch hash, path-scope result, gate logs (command, exit, whether the failure was compile or assertion, pass/fail counts), and a human verdict `code-bug | test-bug | spec-bug | flake` plus a note.

c3 can help without deciding: re-run once for flake, reverse-apply the stored patch in a scratch worktree, and show the compiler-vs-assertion class. There is no commit to check out (F02-4). `--resume` refuses `done`, so “send the fix back to the same author session” is a new run unless resume policy changes.

## Q4

v1, essential:

- Opt-in `--work-set` linking author and tester runs.
- Store a real patch (full diff of the author’s change). Status and stat cannot feed a tester or a reverse-apply gate.
- Declare scope; **after** the run, compare changed paths and flag violations. Do not pretend engines can enforce it (F02-3).
- Scope rule that matches this repo: `tests/**` only for the tester. Inline `#[cfg(test)]` in `src/` is author-owned and outside the cross-model contract (F02-6, F04-2). State that limit in the record.
- Gate, opt-in, serialized: reverse-apply, then `cargo test -j 2` on the named test, classifying compile-fail as inconclusive and assertion-fail as a bite. One cargo at a time on this host (F04-4, F03-3).
- Coordinator verdict field.

Gold-plating: family hard-refuse, AST-aware cfg(test) editing, auto-routing, telemetry dashboards, making the gate the default.

Interaction: a tester `--of` an in-flight author must wait or be refused (F04-1). Do not weaken “one live work run” into two concurrent agents on one tree. A second run starts only after the author record is sealed. The gate is another cargo job, not another live agent, and still needs the same machine-wide build lock.

## Q5

Sensible pairs are different **plans**, not just different brand names. Roster entries have provider, model, engine, roles, plan — no family (F02-5).

| Author | Tester | Why |
| --- | --- | --- |
| GLM-5.3 | gpt-6.1-sol | Different lab and plan. Best default. |
| GLM-5.3 | Muse spark | Different lab. Muse is unrestricted; do not show it an author diff (F02-7). |
| gpt-6.1-sol | GLM or Muse | Same, swapped. |
| GLM-5.3 | GLM-5.3-flash | Refuse in practice: same family, same z.ai plan. |
| GLM-5.3 | DeepSeek-4.1-flash | Weak. Both use the claude engine; confirm they do not share one monthly cap before calling this diverse. |

Warn by default. Hard-refuse only a configured same-plan or same-model pair. `--model` can keep the endpoint and change the id (F04-3), so a label check alone does not enforce the rule. Put the resolved model id in `run.json` and warn on mismatch.

## Q6

- **Shared cap.** GLM, GLM-flash, and possibly DeepSeek draw one z.ai budget. A default second run doubles spend with no quota API to stop early.
- **Trivial tests.** A test with no assertion, or one that never runs, passes the gate unless c3 requires a failing assertion on the reversed tree.
- **API rewrite.** A sighted or poorly scoped tester can change `src/` so the new tests pass. Post-hoc scope does not see edits inside an existing `#[cfg(test)]` module if the whole `src` file is allowed.
- **Windows.** Two cargo jobs lock `target` (OS error 5) or OOM. The gate must wait until the tree is quiet, use `-j 2`, and retry a lock once.
- **Injection.** Author `report.md` or a future full diff is untrusted input. Quote it as data. Muse with `--unrestricted` is the wrong tester for sighted mode.
- **Dirty baseline.** Reverse-apply is wrong if the author edited files that were already dirty. The gate must use only paths the author run itself changed, or it is inconclusive.
- **Resume fiction.** Every fix is a cold run that re-reads the repo until `done` can accept a coordinator note.

## Requested checks

- **RC1.** Working directory: repo root. Permission: read-only. `rg -n "cfg\(test\)" --glob "src/**/*.rs" -c` and `rg --files tests | wc -l`. Settles F02-6 / F04-2 (inline tests dominate). Budget: 1 minute.
- **RC2.** Working directory: repo root. Permission: read-only. Open `.collab/c3-work/work/01-ZAI-claude-glm-5.3/diff.txt` and `run.json`; confirm there is no unified diff hunk. Settles F02-1. Budget: 2 minutes.
- **RC3.** Working directory: repo root. Permission: workspace-write. After any other cargo exits: `c3 work --task c3-work --resume 01-ZAI-claude-glm-5.3` and confirm refusal for state `done`. Settles F02-2. Budget: 2 minutes.
- **RC4.** Working directory: repo root. Permission: read-only. In `crates/c3-core/src/roster.rs`, confirm no `family` field and whether GLM and DeepSeek share `plan`. Settles F02-5 and the Q5 DeepSeek row. Budget: 5 minutes.
- **RC5.** Working directory: repo root. Permission: read-only. Inspect claude, codex, and muse argv builders for a path-scope flag, and the muse path for `--unrestricted` / no allow-list. Settles F02-3 and F02-7. Budget: 10 minutes.

---

### Findings

- **F06-1** [major] `docs/work-mode.md:74`, `crates/c3/src/work/record.rs:519`, `.collab/c3-work/work/01-ZAI-claude-glm-5.3/diff.txt:1` - The work record stores git status and diff --stat, not a unified patch, so neither a sighted tester nor a reverse-apply validity gate has the artifact the brief assumes. Trigger: Implementing alternative 2 (tester reads the author change) or alternative 4 (reverse-apply in a scratch worktree) from the current record alone. Evidence: read-code: The design describes diff.txt as git status before/after plus diff --stat computed by c3, not a full patch.; assumed: Not re-read this turn; prior finding F02-1 says the writer emits no full git diff. Treated as still open. Verify: Open diff.txt for 01-ZAI-claude-glm-5.3 and grep the record writer for a full `git diff` body. Absence of hunks confirms the claim. Remedy: Persist a patch file (author-owned paths only) in the work record before any --of handoff or reverse-apply gate. Supersedes: F02-1.
- **F06-2** [major] `crates/c3/src/work/mod.rs:869`, `docs/work-mode.md:160` - --resume refuses runs that are not quota, timeout, or waiting_quota, so a finished author session cannot receive a test-failure fix and every iteration is a cold run. Trigger: A cross-model test fails and the coordinator tries to return the fix to the same author session. Evidence: assumed: Not re-read this turn. F02-2 reports --resume refuses state done. No later design note in the brief says this changed. Verify: Run `c3 work --task c3-work --resume 01-ZAI-claude-glm-5.3` and confirm a 'state done, not resumable' refusal. Remedy: Either allow a sealed done run to resume with a coordinator note and the tester record, or drop 'same session' from the position and cost a fresh run in the design.
- **F06-3** [major] `docs/work-mode.md:41`, `docs/work-mode.md:234` - No current engine accepts a path allowlist at launch (claude acceptEdits, codex workspace-write, muse unrestricted), so a test-file guard can only be a post-hoc check of files changed. Trigger: Implementing path scope for alternatives 2 or 3 as launch-time containment. Evidence: assumed: Not re-read this turn. F02-3 reports the three argv builders have no path-scoping flag. The brief still calls it a path guard. Verify: Inspect the claude, codex, and muse argv builders for a path-scope flag. None should exist. Remedy: Document scope as post-hoc. Flag out-of-scope edits in run.json. Do not describe it as containment. Prefer a non-mutating tester patch that c3 applies.
- **F06-4** [blocker] `docs/work-mode.md:113`, `.collab/c3-work/work/01-ZAI-claude-glm-5.3/diff.txt:1` - A fail-on-base validity gate is undefined here: c3 never commits, baselines are often dirty, and the record has no patch to reverse-apply. A non-zero cargo test exit also conflates compile errors with assertion failures. Trigger: Running alternative 4 on a task whose tree was dirty before the author ran, or on a test that references a new API. Evidence: read-code: The brief says C3 never commits and the gate may revert the author change in a scratch worktree. It does not say how compile-fail is classified, or where the patch comes from.; assumed: F02-4 and F03-1: no git ref holds the pre-change state; cargo test's exit code does not distinguish compiler errors from assertion failures. Verify: Show a git ref of the author's pre-change tree for a dirty task (none expected). Separately, cargo test a file that names a missing function and compare stderr class to an assertion failure. Remedy: Store a patch. Reverse-apply only author-touched paths in a scratch worktree with its own target dir. Treat compile-fail as inconclusive and assertion-fail as a bite. Keep the gate opt-in and serialized. Supersedes: F02-4, F03-1.
- **F06-5** [major] `crates/c3-core/src/roster.rs:66`, `docs/work-mode.md:19` - The roster has no family field. Same-family refusal cannot be implemented from the schema, and --model can keep the endpoint while changing the model id. GLM with GLM-flash is the pair that shares family and z.ai plan; GLM with DeepSeek may share the claude engine and the same monthly cap. Trigger: Enforcing different-family for author/tester, including a --model override on the author's provider. Evidence: assumed: F02-5: validate_roster has no family field; entries have provider, model, engine, roles, plan, ext. F04-3: --model override bypasses a label check. Not re-read this turn. Verify: Grep roster schema and validate_roster for family. Attempt a same-family pair via --model and record whether c3 refuses or warns. Check whether DeepSeek and GLM share one plan id. Remedy: Add an optional family (or plan-group) field. Warn on mismatch. Hard-refuse only configured same-plan or same-model pairs. Record the resolved model id. Supersedes: F02-5, F04-3.
- **F06-6** [major] `crates/c3/src/work/mod.rs:1751`, `docs/work-mode.md:234` - Most tests in this repo are inline #[cfg(test)] modules inside src files. A tests/-only scope forces integration tests, and an author barred from tests/ can still add or weaken inline tests. Allowing src/ lets the tester edit the implementation. Trigger: Applying alternative 3 ('author may not edit tests; tester may only edit tests') to a change whose natural tests are unit tests in src. Evidence: assumed: F02-6 and F03-2 and F04-2 agree src #[cfg(test)] dominates tests/. Not re-counted this turn. Verify: Count #[cfg(test)] under src versus files under tests/. Expect src to dominate. Remedy: v1 scope is tests/** only, and the record states that inline unit tests are outside the cross-model contract. Do not claim a path check covers cfg(test) edits. Supersedes: F02-6, F03-2, F04-2.
- **F06-7** [major] `docs/work-mode.md:253`, `docs/work-mode.md:60` - Feeding author report or diff text to a tester is an injection and exfiltration path. On Muse the worker is --unrestricted (shell on, no allow list), so injected instructions can run arbitrary commands. Claude-engine testers stay inside the cargo allow list. Trigger: A sighted Muse tester reads an author diff or report that contains instructions or secret-shaped text. Evidence: assumed: F02-7 and F04-5: Muse work runs have no allow list or sandbox; the record would forward author content into the tester prompt unsanitized. Not re-read this turn. Verify: Confirm the muse argv has no allow-list. Feed a canary instruction in a synthetic author diff and see whether it is quoted as data or omitted. Remedy: Do not use Muse for sighted mode. Pass author content as a quoted, untrusted attachment, or pass only a c3-generated patch with prose stripped. Supersedes: F02-7, F04-5.
- **F06-8** [major] `docs/work-mode.md:27` - The one-live-work-run-per-task guard blocks a tester --of an author while that author is running or waiting_quota. A validity gate that launches cargo beside another build also breaks the host -j 2 / no-parallel-cargo rule. Trigger: Starting a tester against a live author, or running the gate while any cargo build holds the machine. Evidence: read-code: The brief states one live work run per task, and full cargo test only with -j 2 and never beside another build.; assumed: F04-1: the admission guard refuses a second work run on the task. F04-4 and F03-3: unsound gate under OOM and Windows OS error 5 if cargo is not serialized. Verify: Start an author detached, then attempt a tester on the same task. Separately, run two cargo test -j 2 processes back to back on Windows and watch for OOM or OS error 5. Remedy: Keep one live agent per task. Start the tester only after the author record is sealed. Take a machine-wide cargo lock for the gate; retry a file lock once; classify a second failure as flake only after that retry. Supersedes: F04-1, F04-4, F03-3.
- **F06-9** [note] `.collab/consilium-2026-10-29-test-workers/handoffs/01-claude-cross-model-tests.md:43` - Blind testing against the author brief is not an independent oracle: both models share one document, so a misread spec produces agreeing tests and code. Trigger: Alternative 3 run where the tester's only spec is the same brief.md the author used. Evidence: read-code: Alternative 3 says the tester writes tests from the SPEC before seeing the implementation. The task record's spec artifact is brief.md, which the author already consumed. No second frozen contract is specified. Verify: Check whether a work record has any spec artifact other than brief.md. If not, a blind tester and the author share one input. Remedy: Require an immutable contract file hashed into both run records, or call the mode brief-conditioned rather than independent.

### Prior findings

- F02-1 - still-open - Merged forward into finding #1. diff.txt is still status plus stat; no full patch is stored.
- F02-2 - still-open - Resume of a done author run is still refused. Restated as finding #2, not a new claim.
- F02-3 - still-open - No engine path allowlist. Restated as finding #3.
- F02-4 - still-open - Superseded by finding #4, which merges the dirty-baseline gap with compile-vs-assertion (F03-1).
- F02-5 - still-open - Superseded by finding #5 together with the --model bypass (F04-3).
- F02-6 - still-open - Superseded by finding #6 together with F03-2 and F04-2. Inline cfg(test) still defeats a tests/ path scope.
- F02-7 - still-open - Superseded by finding #7 together with F04-5. Muse sighted handoff is still the injection sink.
- F03-1 - still-open - Superseded by finding #4. Exit code of cargo test still conflates compile errors and assertion failures.
- F03-2 - still-open - Superseded by finding #6. The tests/ versus src cfg(test) dilemma is unchanged.
- F03-3 - still-open - Superseded by finding #8. Windows target locking is part of the serialization requirement.
- F04-1 - still-open - Superseded by finding #8. One live work run still blocks a paired tester.
- F04-2 - still-open - Superseded by finding #6. Path scope still cannot see inline cfg(test) edits.
- F04-3 - still-open - Superseded by finding #5. --model can still retarget family on the same endpoint.
- F04-4 - still-open - Superseded by finding #8. The gate is still unsound without a machine-wide cargo lock and a flake retry.
- F04-5 - still-open - Superseded by finding #7. Author content forwarded into the tester prompt is still untrusted.
- F05-1 - not-checked - Bake-off output shape (empty locations). Outside this design question; those JSON files were not opened this turn.
- F05-2 - not-checked - prior_findings empty on glm-2 and mimo chunks. Not opened this turn.
- F05-3 - not-checked - grok-1 tool-trace claim. Session log not opened this turn.
- F05-4 - not-checked - Q5 rubric versus verdict. This consultation itself uses ADVISE as required by the schema. The scoring rubric file was not re-read.
- F05-5 - not-checked - Coordinator second-pass versus worker payload. Run records were not diffed this turn.

## Verdict: ADVISE

Do not make cross-model testing the default c3 pipeline until a patch is stored, author resume exists, and the gate can tell a red test from a compile failure.

### Blockers

- **F06-4** `docs/work-mode.md:113`, `.collab/c3-work/work/01-ZAI-claude-glm-5.3/diff.txt:1` - A fail-on-base validity gate is undefined here: c3 never commits, baselines are often dirty, and the record has no patch to reverse-apply. A non-zero cargo test exit also conflates compile errors with assertion failures. Verify: Show a git ref of the author's pre-change tree for a dirty task (none expected). Separately, cargo test a file that names a missing function and compare stderr class to an assertion failure. Remedy: Store a patch. Reverse-apply only author-touched paths in a scratch worktree with its own target dir. Treat compile-fail as inconclusive and assertion-fail as a bite. Keep the gate opt-in and serialized.

### Unproven scenarios

- Exact #[cfg(test)] versus tests/ counts were not recounted this turn.
- Whether DeepSeek-4.1-flash shares the z.ai monthly plan with GLM was not confirmed from roster data.
- No paired author/tester run exists, so bug-finding power of blind versus sighted is unmeasured.
- Windows OS error 5 and OOM were not reproduced here.
- F05-1 through F05-5 were not re-verified against the output JSON files.

### First-run checklist (observable)

_(none)_
