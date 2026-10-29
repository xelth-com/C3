# Handoff 04 - Meta Muse (muse): cross-model-tests-meta

Date: 2026-10-29 17:11 local. Author: Meta Muse (muse) (model muse-spark-1.3-contributor, effort high), muse-cli 1.4.14-R7309.1.
Reviewer: meta :: muse-spark-1.3-contributor [muse] (provider from roster, model from roster; engine muse (C:\Users\Dmytro\AppData\Local\Programs\muse\muse.cmd); provider fingerprint 1c6f62bb040d; harness muse-cli 1.4.14-R7309.1).
Preflight: ok: signed in (~/.config/muse/auth.json: providers.meta, mechanism oauth).
Effort: high sent (requested high, mapping muse-v1, by caps-v1: engine:muse, muse-spark-1.3-contributor; not confirmed by the provider). Consultation id: 428c4a92-0997-4acc-a349-959adc154cdd.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `muse exec --json --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-aa70c9dc336d4eeea061d6b8976374da.txt --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json --model muse-spark-1.3-contributor --reasoning-effort high --no-foreign-personal-context --disable-web-tools --disable-write --disable-shell --approval-mode never` (prompt from a file: --prompt-file).
Parent thread: (none - new thread). Result thread: `01a1886c-80c3-74f2-834d-0494648c7d12` (source: events).
Brief: `.collab/consilium-2026-10-29-test-workers/handoffs/01-claude-cross-model-tests.md` (sha256 57c053d8e8cc). Reviewed: 0b712c4, base 0b712c46cf717bdb90de5e0228b720d17388cfb9, tree sha256 fbb90a090f31, 0 changed files.
Bridge outcome: usable reply. Wall time: 142.6 s. Tokens: not reported by muse.
Engine turns: 1 (each one a Muse Code subscription prompt); MSP schema_version 1.
Warnings: routing: no lab known for #14 grok :: grok-4.7 [grok] (its model id has no known vendor prefix) - it counts as a lab of its own; give its roster entry a "lab"; muse: warning: rules file at \\?\C:\Users\Dmytro\C3\AGENTS.md exists, but the workspace is untrusted, so it is skipped for this session; restart with --trust-workspace to load project rules.
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - Advise convention-plus-linked-record first; do not lock a universal paired-test pipeline into c3 yet. Findings: 0 blocker, 4 major, 1 minor, 0 note (F04-1..F04-5, tracked in `findings.json`). Structured reply: `handoffs/04-muse-cross-model-tests-meta.reply.json`.
Raw event stream: `handoffs/04-muse-cross-model-tests-meta.events.jsonl`.
Verbatim reply follows.

---

## Framing — challenge first

The brief assumes every worker task gets the same default pipeline, so the choice is once-and-for-all. Reject that: make cross-model tests risk-tiered, not default. Most chores do not repay 2–3x model cost plus two serial full test suites on a `-j 2`, OOM-constrained Windows host.

### Options the brief does not list

- **Audit, not gate:** tester runs post-hoc, records signal, never blocks the author landing.
- **Test-plan-first:** small cross-model test plan reviewed by coordinator, then author writes code + tests.
- **Same-model tests + cross-model review:** keep diversity in review/oracle, not in test authorship.
- **Sampled enforcement:** convention by default, c3 linkage + gate only for flagged high-risk tasks.
- **Competitive oracle:** two blind test suites from two families; disagreement itself is the signal.

### What I need to choose

Escape rate of author-only tests, flake rate on Windows, real cost per paired task on capped plans, and how often author/tester disagree. Without those, do not lock a universal default.

## Q1 — Build into c3?

No, not as the universal default. Start with convention-only (Alt 1) plus linked records done by hand, and build c3 support only after paired runs show value. Evidence that changes this: blind testers repeatedly catch bugs the author suite misses, at acceptable cost/flake. If built, build linkage + scope + audit trail first, gate second.

## Q2 — Blind vs sighted?

Blind catches wrong-code bugs (independent reading of spec); sighted catches integration/edge bugs faster. Blind fails on vague specs (tests a misread spec, both sides argue). Sighted fails by overfitting (tests parrot the implementation, pass trivially). Prefer brief's split: blind for clear contracts, sighted for bugfix/refactor — but keep blind out of `src`-embedded tests.

## Q3 — Who decides code-bug vs test-bug?

Only the coordinator decides. c3 must not decide; it assists: re-run tester on base (revert-check), re-run author suite, record both logs, and store a coordinator verdict field (`upheld-test / upheld-code / unclear`) with rationale. Auditable record: linked run ids, both diffs, base/after gate logs, spec version, decider + timestamp.

## Q4 — Minimal v1 surface?

Essential: run linkage (`--of` + paired record dir), sequential lease fixing the one-live-run clash, coarse path scope (`tests/` only), coordinator verdict field. Defer: automatic validity gate, hunk-level `#[cfg(test)]` enforcement, auto-arbitration. The `#[cfg(test)]` case cannot be enforced by paths — exclude it from blind-mode v1 or accept directory-level only.

## Q5 — Family pairings?

Sensible: GLM author with gpt-6.1-sol or Muse tester and vice versa; DeepSeek-flash as cheap tester, not as sole author-oracle for risky code. Do not hard-enforce in v1 — warn only. Enforcement needs a real roster `family` field; labels plus `--model` override are spoofable, and hard refusal will misfire.

## Q6 — Unlisted risks?

Cost multiplication on monthly caps (2–3 full runs + gate suites), trivially-passing tests without a bite-check, tester rewriting the API instead of testing it, Windows flake/OOM false reds, prompt-injection via author diff into tester, secret carry-over through verbatim `events.jsonl`, and lock-in: a universal default is expensive to undo.

## Requested checks

- RC1 (finding #4): in repo root, read-only, run `cargo test -p c3-cli --test work -- a_second_work --nocapture` approx 5 min; settles whether paired runs are refused today.
- RC2 (finding #2): read-only, grep `#[cfg(test)]` under `crates/` scope 10 min; settles how much test code lives in `src` vs `tests/`.
- RC3 (finding #3): read-only, inspect roster schema + `select_worker` for a `family` field 15 min; settles enforce-vs-warn feasibility.

---

### Findings

- **F04-1** [major] `docs/work-mode.md:27` - The 'one live work run per task' admission guard directly blocks paired author/tester runs unless the minimal surface redesigns it. Trigger: Running tester --of an author while the author is still running or waiting_quota. Evidence: read-code: Admission refuses a second c3 work on same task while one runs; liveness treats running/waiting_quota as live.; read-code: Brief states nothing in c3 knows two runs belong together and flags interaction with one-live-run rule as open. Verify: Start author run detached, then attempt tester run on same task; confirm intended refuse/allow and status output. Remedy: Define paired-run lifecycle for v1: sequential lease (author must be done/quota before tester starts) with linked record, or explicit concurrent slots; refuse tester --of a live author with a clear message.
- **F04-2** [major] `.collab/consilium-2026-10-29-test-workers/handoffs/01-claude-cross-model-tests.md:39` - A path-scope guard limited to tests/ cannot enforce 'author may not edit tests / tester may only edit tests' because #[cfg(test)] modules live inside src files. Trigger: Blind-mode task where the contract is tested by unit tests inside src/. Evidence: read-code: Brief itself flags #[cfg(test)] modules inside src files as a problem for the path guard.; read-code: Worker preamble allows touching files the brief 'clearly implies' and diff.txt only records git status/diff-stat, with no per-hunk or per-file-role attribution. Verify: Run a paired task with a src #[cfg(test)] test; inspect diff.txt/run.json to see whether the author/tester edit is attributed or missed. Remedy: For v1 enforce directory scope only (tests/ plus new test files); treat src-embedded tests as out of scope for blind mode or require hunk-level check; document the gap.
- **F04-3** [major] `docs/work-mode.md:19` - 'Different family' enforcement by provider/model label is bypassable via --model override, which keeps the endpoint but changes the model id. Trigger: Coordinator requests tester on same provider with --model pointing at the author's family. Evidence: read-code: --model overrides the entry model id on the same endpoint; roster selection keeps endpoint.; read-code: Brief proposes c3 refuses a same-family tester but roster lists GLM/GLM-flash/DeepSeek on one engine plus codex/muse engines with no family field. Verify: Inspect roster schema and select_worker validation; attempt same-family pair via --model override and confirm refuse vs warn. Remedy: Add an explicit family field to the roster (not derived from provider label) and validate author/tester pair against it; log both families in run.json linkage.
- **F04-4** [major] `.collab/consilium-2026-10-29-test-workers/handoffs/01-claude-cross-model-tests.md:25` - The fail-on-base / pass-after validity gate is unsound on this host without serialisation and flake handling: full cargo test needs -j 2 and never beside another build (OOM). Trigger: Validity gate run while another cargo build holds the machine, or a flaky Windows test. Evidence: read-code: Constraints state Windows host, -j 2 only, never beside another build, and no quota API on z.ai/Muse/Grok.; read-code: First GLM run needed whole-workspace cargo test plus clippy/fmt to go green, showing gate cost is a full suite per leg. Verify: Run the gate twice on a known-green task with -j 2 serialised; check for OOM/flake false reds in the recorded logs. Remedy: Make the gate opt-in per task, run base/after checks serially in a scratch worktree with -j 2, retry once, and record attempt logs; never auto-gate every worker task.
- **F04-5** [minor] `docs/work-mode.md:60` - Passing the author's diff into a sighted tester creates a prompt-injection and secret-exfiltration path that the current record design widens. Trigger: Author output containing instructions or secret-shaped text forwarded into tester prompt. Evidence: read-code: events.jsonl is retained verbatim, unsanitized, and may contain whatever the child printed including credentials; run.json/report/diff are sanitized.; read-code: Sighted tester consumes author brief plus diff; brief lists prompt-injection through author diff as an unlisted risk to assess. Verify: Feed a canary instruction/secret in a synthetic author diff through the tester handoff and check whether it reaches the tester prompt or record unsanitized. Remedy: Sanitize/scope what crosses runs (diff-stat plus test-relevant hunks only, secret-scan before handoff), and document that tester events remain unsanitized.

### Prior findings

_(none)_

## Verdict: ADVISE

Advise convention-plus-linked-record first; do not lock a universal paired-test pipeline into c3 yet.

### Blockers

_(none)_

### Unproven scenarios

- Whether blind testers on this roster actually catch more escaped defects than sighted testers — no paired-run data exists yet.
- True monthly cost and quota-exhaustion frequency of a universal paired pipeline on z.ai/Muse/Grok capped plans.

### First-run checklist (observable)

_(none)_
