# Handoff 03 - Gemini (agy): cross-model-tests-gemini

Date: 2026-10-29 17:14 local. Author: Gemini (agy) (model gemini-3.8-flash-high, effort tier in the model id), agy-cli (version unknown).
Reviewer: gemini :: gemini-3.8-flash-high [agy] (provider from roster, model from roster; engine agy (C:\Users\Dmytro\AppData\Local\Microsoft\WinGet\Packages\Google.AntigravityCLI_Microsoft.Winget.Source_8wekyb3d8bbwe\agy.exe); provider fingerprint a612ba12e63e; harness agy-cli (version unknown)).
Preflight: ok: signed in (6 models).
Effort: nothing sent (requested high, mapping model-tier, by caps-v1: engine agy, the tier is part of the model id; not confirmed by the provider). Consultation id: 85606e82-e3aa-4b03-b9a5-298628527419.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `agy -p= --input-format stream-json --output-format stream-json --model gemini-3.8-flash-high --json-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json --print-timeout 0 --sandbox --disable-slash-commands` (prompt on stdin as one NDJSON line).
Parent thread: (none - new thread). Result thread: `9f1c719b-0826-4be8-bcc0-b5c1a2d5358a` (source: events).
Brief: `.collab/consilium-2026-10-29-test-workers/handoffs/01-claude-cross-model-tests.md` (sha256 57c053d8e8cc). Reviewed: 0b712c4, base 0b712c46cf717bdb90de5e0228b720d17388cfb9, tree sha256 fbb90a090f31, 0 changed files.
Bridge outcome: usable reply. Wall time: 300.6 s. Tokens: in 114357 (cached 759606), out 24559, reasoning 21288.
Engine turns: 1.
Warnings: routing: no lab known for #14 grok :: grok-4.7 [grok] (its model id has no known vendor prefix) - it counts as a lab of its own; give its roster entry a "lab".
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - Keep cross-model testing as a coordinator convention with minimal metadata linkage in c3; do not hardwire pipeline enforcement or validity gates into the binary until empirical value and Rust compilation dynamics are proven. Findings: 0 blocker, 2 major, 1 minor, 0 note (F03-1..F03-3, tracked in `findings.json`). Structured reply: `handoffs/03-agy-cross-model-tests-gemini.reply.json`.
Raw event stream: `handoffs/03-agy-cross-model-tests-gemini.events.jsonl`.
Verbatim reply follows.

---

## Framing and Unlisted Options

The brief frames the decision as choosing between manual convention or building an enforced author/tester pipeline into `c3 work`. This framing overlooks three critical options:

1. **Option A: Adversarial Review Panel (`c3 consult --purpose test-oracle`)**: Instead of granting write tools to a second worker, run a read-only consultation. Reviewers inspect the author's diff and synthesize test cases or patches as structured findings. This avoids git/cargo lock races and respects strict sandbox boundaries.
2. **Option B: Coordinator Skill Orchestration (`plugin/skills/coordinate`)**: Keep `c3 work` as an atomic worker execution engine. Let the Claude Code coordinator skill or an external script orchestrate sequential runs, branch switching, and scratch worktrees. C3 should not become a complex workflow scheduler.
3. **Option C: Mutation and Property Testing (`cargo-mutants` / `proptest`)**: Two LLMs reading the same ambiguous prompt often share the same misconceptions. Verifying the author's tests via property-based testing or mutation analysis provides a genuinely independent oracle.

To choose between these, we need to know: (a) task distribution (novel features vs bug fixes/refactors), (b) baseline test failure modes in single-worker runs, and (c) Windows host capacity for sequential cargo builds without target-lock contention (`OS error 5`).

## Numbered Questions

### Q1. Value vs Convention
Not worth building into C3 in v1. Keep it as coordinator convention (Alternative 1) with minimal metadata linkage. Building an enforced pipeline into the C3 binary prematurely freezes workflow semantics before empirical value is demonstrated.
*Evidence to change answer*: (a) Data showing coordinators frequently fail to run testers manually, (b) 20+ benchmarked tasks showing cross-model testers catch severe defects missed by GLM and clippy, and (c) API compilation failure between author and tester is under 10%.

### Q2. Blind vs Sighted Tester
- **Sighted tester**: Catches implementation edge cases and unhandled boundary paths. Fails by overfitting to the author's code, asserting existing behavior rather than specified behavior.
- **Blind tester**: Tests true specification requirements without implementation bias. However, in Rust, it fails catastrophically at compilation: without shared type signatures, method names, and error enums, the tester invents incompatible APIs that fail `cargo test --no-run`.
- **Verdict**: Sighted catches more runnable bugs in Rust. Blind testing is only viable when the brief includes frozen, copy-pasteable Rust interface definitions.

### Q3. Bug Arbitration and Auditing
- **Arbitrator**: The human coordinator must arbitrate. LLMs cannot impartially resolve their own specification disputes.
- **Auditable Record**: C3 must record `parent_run_id`, separate stdout/stderr streams for compilation vs assertion failures, exact diffs of both runs, and base-revert test exit codes.
- **C3 Assistance**: C3 can run an automated revert check (run test on base commit) and classify failures into `CompilationError` vs `AssertionFailure`, presenting clean diagnostics without deciding blame.

### Q4. Minimal C3 Surface
- **Essential for v1**: Record linkage (`--parent-run <id>` in `run.json`) and advisory path scoping (`--path-scope "tests/**"` reporting warnings on violations).
- **Gold-plating**: Automatic scratch worktrees, hardcoded role state machines, and automated arbitration gates.
- **One live run**: Sequential execution (author completes before tester launches) preserves the "one live run per task" invariant naturally.
- **`#[cfg(test)]` modules**: Path scoping cannot safely inspect AST boundaries inside `src/`. Restrict tester to `tests/**` (integration tests only) in v1.

### Q5. Family-Diversity Rules
- **Pairings**: GLM-5.3 (Zhipu/Claude engine) pairs effectively with DeepSeek-4.1-flash (BytePlus/Claude engine) or GPT-6.1-sol (OpenAI/Codex engine). Muse is undesirable due to its `--disable-sandbox` requirement on Windows.
- **Enforcement**: Warn only. Hard refusal prevents offline development, single-subscription users, and tasks where fallback models are unavailable.

### Q6. Unlisted Risks
1. **Compilation Masking**: Naive validity gates treat compilation failure on base commit as test failure, allowing broken tests to pass the gate (finding #1).
2. **`Cargo.toml` Pollution**: The tester adding dependencies, mutating lockfiles.
3. **Windows File Locks**: Sequential cargo runs locking `target/debug/` binaries with OS error 5 (finding #3).
4. **Mocking Trap**: Blind testers stubbing internal logic with dummy mocks, testing nothing of value.

## Requested checks

- **RC1**: Run `cargo test --test work -- --exact nonexistent_symbol` in `crates/c3-cli` (permission: read-only, budget: 60s). Observation that settles finding #1: confirms `cargo test` exit code when tests execute with 0 failures vs when compilation fails, validating whether C3 can differentiate validity gate outcomes without stderr parsing.

---

### Findings

- **F03-1** [major] `.collab/consilium-2026-10-29-test-workers/handoffs/01-claude-cross-model-tests.md:43` - A naive validity gate checking non-zero exit code of `cargo test` on base commit conflates compiler errors with test assertion failures. Trigger: A tester worker writes a test using newly introduced types or functions, or produces invalid syntax, and C3 runs `cargo test` against base commit. Evidence: read-code: Alternative 4 specifies that tests must fail on the base commit without differentiating compile failure from test execution failure.; inferred: Rust cargo test exits non-zero (exit code 101) when compilation fails, meaning syntax errors and missing types pass a naive failure check. Verify: Run `cargo test` on a test file referencing a nonexistent function on base commit and inspect exit code and stderr. Remedy: Require a two-phase gate: compile test with `--no-run` against the new change first, and only consider a base run valid if compilation succeeds or failure is explicitly an assertion failure (`failures:` in output).
- **F03-2** [major] `.collab/consilium-2026-10-29-test-workers/handoffs/01-claude-cross-model-tests.md:38` - Path scoping to test files creates an irreconcilable dilemma between black-box integration tests and in-tree `#[cfg(test)]` modules. Trigger: Enforcing path scope to `tests/` prevents writing unit tests for private functions in `src/`, while allowing `src/` permits tester to modify author implementation. Evidence: read-code: Lines 38-39 note that `#[cfg(test)]` modules live inside src files.; read-code: Work runs track file changes at the file path level via `git diff --stat` and `git status`, with no AST awareness. Verify: Review the workspace tests: over 80% of unit tests in `c3` and `c3-core` are inline `#[cfg(test)]` modules inside `src/`. Remedy: Explicitly scope cross-model tester workers to integration tests under `tests/` in v1, leaving unit testing to the author model.
- **F03-3** [minor] `.collab/c3-work/work/01-ZAI-claude-glm-5.3/report.md:29` - Rapid sequential test worker runs on Windows risk target directory binary file-locking contention (OS error 5). Trigger: Running an author worker followed immediately by a tester worker and validity gate checks on the same Windows host. Evidence: read-code: Real GLM work run report records: 'failed to remove target\debug\c3.exe (os error 5) (a stale lock)' during rebuild. Verify: Execute two rapid sequential `cargo test -j 2` runs after binary modification on Windows and monitor for OS error 5. Remedy: Add retry loops around target cleanup or use separate `--target-dir` scratch directories for tester validity checks.

### Prior findings

_(none)_

## Verdict: ADVISE

Keep cross-model testing as a coordinator convention with minimal metadata linkage in c3; do not hardwire pipeline enforcement or validity gates into the binary until empirical value and Rust compilation dynamics are proven.

### Blockers

_(none)_

### Unproven scenarios

_(none)_

### First-run checklist (observable)

_(none)_
