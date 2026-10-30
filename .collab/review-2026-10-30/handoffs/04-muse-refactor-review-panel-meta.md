# Handoff 04 - Meta Muse (muse): refactor-review-panel-meta

Date: 2026-10-30 14:52 local. Author: Meta Muse (muse) (model muse-spark-1.3-contributor, effort xhigh), muse-cli 1.4.14-R7309.1.
Reviewer: meta :: muse-spark-1.3-contributor [muse] (provider from roster, model from roster; engine muse (C:\Users\Dmytro\AppData\Local\Programs\muse\muse.cmd); provider fingerprint 1c6f62bb040d; harness muse-cli 1.4.14-R7309.1).
Preflight: ok: signed in (~/.config/muse/auth.json: providers.meta, mechanism oauth).
Effort: xhigh sent (requested xhigh, mapping muse-v1, by caps-v1: engine:muse, muse-spark-1.3-contributor; not confirmed by the provider). Consultation id: c1c00a6d-637d-4a70-aefa-8b434e0cbfe8.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: core-contract). Argv: `muse exec --json --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-8786ccbca147491fb609509029f9198c.txt --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json --model muse-spark-1.3-contributor --reasoning-effort xhigh --no-foreign-personal-context --disable-web-tools --disable-write --disable-shell --approval-mode never` (prompt from a file: --prompt-file).
Parent thread: (none - new thread; no thread of lineage meta :: muse-spark-1.3-contributor in this task's ledger; other lineage(s): openai :: gpt-6-astra). Result thread: `01a18d11-f478-7980-b075-6f1d475f9782` (source: events).
Brief: `.collab/review-2026-10-30/handoffs/01-claude-refactor-review.md` (sha256 0d3890e5139c). Reviewed: 958aa5b, base 958aa5b79d955e28e328a91c75f8856d3406a0da, tree sha256 78368e63f658, 0 changed files.
Bridge outcome: usable reply. Wall time: 190 s. Tokens: not reported by muse.
Engine turns: 1 (each one a Muse Code subscription prompt); MSP schema_version 1.
Warnings: routing: no lab known for #14 grok :: grok-4.7 [grok] (its model id has no known vendor prefix) - it counts as a lab of its own; give its roster entry a "lab"; muse: warning: rules file at \\?\C:\Users\Dmytro\AppData\Local\Temp\c3-review\AGENTS.md exists, but the workspace is untrusted, so it is skipped for this session; restart with --trust-workspace to load project rules.
Timeout: 2400 s (the default of purpose core-contract); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - Advise: binary secret bypass, agy classifier bypass, claude warn-only with hooks, and two fail-open scope/brief paths need fixes before 0.2.4. Findings: 0 blocker, 3 major, 4 minor, 0 note (F04-1..F04-7, tracked in `findings.json`). Structured reply: `handoffs/04-muse-refactor-review-panel-meta.reply.json`.
Raw event stream: `handoffs/04-muse-refactor-review-panel-meta.events.jsonl`.
Verbatim reply follows.

---

# Core-contract review (base 0b712c4 → head 958aa5b)

Shell was disabled, so no `git diff --stat` / `grep -n` was run; all line refs are from direct file reads. Not read: full diff hunks, `pack/redact.rs`, `health.rs`/`quota.rs` call sites of `retention.rs`, `verdict.rs` ordering, full `panel/run.rs` re-seat loop, `consult/detach.rs` judgement/liveness, MCP `c3_finish_task` arg mapping.

## Q1. `c3 finish` — can it commit what it must not?

Staging is `git add -A` (`finish/git.rs:33`), then `paths_to_unstage` (`finish/never_commit.rs:48`) with POSIX normalisation (`:65`), then `scan_staged` over `ACMR` added lines (`finish/secret_gate.rs:57`). Added-only is a defensible trade-off for fixture re-flags (existing test `gate_ignores_an_unchanged_fixture_line_but_catches_a_new_secret`, `:187`), since an already-committed secret needs no new block. Two residual holes remain outside the two already-known gate issues: binary blobs are skipped entirely (finding #1, `:37-54`), and the never-commit set is narrow/case-sensitive (`never_commit.rs:14-23`; `Glob::new` without the case-insensitive builder `discover.rs` uses). Staged-outside-repo was not observed: every git call is `git -C repo_root` (`git.rs:8`, `secret_gate.rs:98-128`).

## Q2. Drift relaxations — hole for a writing reviewer?

Yes for two paths. Claude is warn-only despite hooks still applying (`tree.rs:23-24,53-62,171-205`, finding #3). The collab ignore list (other tasks, `.consult.*`, ledger files, sibling handoffs: `tree_check.rs:93-150`; `tree.rs:114-148`) is a usability need but means writes there never fail (finding #4). Agy's classifier is substring-based and bypassable (finding #2, `tree_check.rs:179-188`).

## Q3. “Pure” refactors

`retention.rs:140-198` centralises windows/precedence with a kind-lookup (`:192-198`) guarding reorder desync — sound shape. Behaviour-neutrality could not be fully verified read-only: `health.rs`/`quota.rs`/`verdict.rs` call sites, `Minutes` vs `OutWindow` boundary (`is_live`, `:266-276`), and `clears` semantics (`:293-299`) need a diff of the moved constants. `orchestrate/` split and `render→section` rename were not traced to callers; treat as unproven.

## Q4. Re-seat + per-model mark

Reserve is the same seeded draw continued past seats (`plan.rs:694-708`; `routing.rs:603-652` slot `seats+reserve+1`), so reproducibility holds given the same seed. Promotion on before-start preflight failure (`run.rs:1106-1264`) appends reserve runners; double-seat (same NN twice) was not observed in the lines read but the full seat/NN assignment was not read — left as unproven. Per-model mark (`health.rs:145-197`, 24 h, same-model clear) is correctly narrower than provider marks.

## Q5. Death records — false “died” race?

Work path is locked + generation-checked (`record.rs:183-242`, `is_live` grace 120 s `:315-338`). Consult reader path is lock-free with only a re-read check (`detach.rs:1207-1243`, grace 60 s `detached.rs:20-21`) — benign last-writer-wins in practice but weaker (finding #6). `vanished` maps unknown engine evidence to `bridge`/`unknown` (`detached.rs:208-216`); a live survivor with no engine evidence reads as bridge-vanished by design.

## Q6. Other 0.2.4 blockers?

`PRIORS_KEYS` now pins `xelth-2026-1` (`router/sign.rs:23-26`) with correct pinned-first lookup (`:79-91`) — new trust root, confirm rotation plan out of band. `scope_violations` fail-open (finding #5) and grok empty-inline fail-open (finding #7) should block. No other contract break was proven in the lines read.

## Requested checks

- RC1 (read-only): unit-check `agy_tool_is_writing` on `write_read_cache`, `search_and_edit`, `edit_file`, `apply_patch` (finding #2).
- RC2 (workspace-write): stage a NUL-containing blob with a test key, run secret gate, confirm refused (finding #1).
- RC3 (read-only): call `scope_violations` with scope `['[bad']` and one changed path; confirm error vs empty (finding #5).

---

### Findings

- **F04-1** [major] `crates/c3/src/finish/secret_gate.rs:36`, `crates/c3/src/finish/secret_gate.rs:51` - c3 finish secret gate skips any staged blob with a NUL in the first 8 KiB, so a secret committed as or inside a binary (PEM/DER, .p12/.pfx, image metadata, vendored binary fixture) passes clean. Trigger: Stage a new binary file containing a secret-shaped string (or a text secret renamed to a binary-detected blob) and run c3 finish. Evidence: read-code: scan_staged_text returns None for binary; is_binary checks first 8 KiB for NUL; scan_staged only scans added text lines so binary diffs contribute nothing. Verify: Stage a fixture binary with an embedded sk-test key plus NUL byte; run the gate and confirm clean vs refused. Remedy: Fail closed on binary adds: refuse binary adds by default (or gate by hard-ignore basename list plus size cap) with --allow-secret as the override; document the binary carve-out.
- **F04-2** [major] `crates/c3/src/engines/tree_check.rs:179` - agy read-only decision is a substring allow-list: any tool name containing read/view/list/grep/glob/search/find/cat/ls is read-only, so a writing tool whose name contains one of those substrings warns instead of failing. Trigger: agy stream names a writing tool such as write_read_cache, broader_search_and_edit, or find_and_replace that contains a read-only substring while the tree changed. Evidence: read-code: READ_ONLY substrings matched with contains() after lowercasing; empty name is writing. No exact tool inventory is checked. Verify: Unit-check agy_tool_is_writing against adversarial names (write_read_cache, search_and_edit, cat_write) and confirm they classify as writing. Remedy: Replace substring match with an exact (or prefix) allow-list of known read-only agy tools plus deny-by-default; log the tool names that decided the outcome.
- **F04-3** [major] `crates/c3/src/consult/orchestrate/tree.rs:53`, `crates/c3/src/consult/orchestrate/tree.rs:23` - claude runs write-disabled yet only warns on any tree/collab/brief/artifact change, while its own note admits managed settings and hooks still apply — a claude reviewer (or its hooks) that did write still returns usable. Trigger: Run claude with a hook that writes a file during the turn; tree check reports warned and the reply stays usable. Evidence: read-code: claude in write-disabled set with muse/grok/http; tree outcome warned for all four; engine note says hooks still apply and check cannot tell who changed it. Verify: Run a claude turn with a write hook fixture and inspect outcome warned vs failed and the recorded TreeNote. Remedy: For claude, fail on change unless hooks are proven off, or record hook inventory in the ledger and fail when hooks were present; at minimum downgrade usable to failed when the change is inside the review scope.
- **F04-4** [minor] `crates/c3/src/engines/tree_check.rs:106`, `crates/c3/src/consult/orchestrate/tree.rs:118` - Collab drift scope silently ignores writes a reviewer could make: other-task directories, .consult.* control files, this task state.md/findings.json/sessions.json and temps, plus own and sibling handoff prefixes — such writes never fail the run. Trigger: Reviewer writes to another task dir, .consult.* file, or sibling handoff path during its turn; compare_collab drops it. Evidence: read-code: compare_collab skips belongs_to_other_task, is_control_file, task-owned ledger files, and caller ignore_prefixes; panel members additionally ignore sibling handoffs and stores.; read-code: Orchestrator builds own/sibling prefixes and passes them as ignored; collab changes there become warnings only via the write-disabled path. Verify: Mutate an other-task file and a .consult.* file between collab snapshots and confirm compare_collab returns empty. Remedy: Keep the ignores for usability but fail when the ignored path was written by the reviewer process (attribute by pid/time) or at least surface ignored-path writes as a separate failed class; document the monitored boundary.
- **F04-5** [minor] `crates/c3/src/work/record.rs:761` - c3 work scope check is post-hoc and fail-open: empty scope checks nothing and an unbuildable glob set returns no violations, so a typoed --scope silently disables the OUT OF SCOPE guard. Trigger: Start c3 work --scope with a typoed glob (e.g. unbalanced [) and change a file outside intent; run exits 0 with no scope_violations. Evidence: read-code: scope_violations early-returns empty on empty scope; invalid glob is skipped and failed build returns empty; only non-matching expanded paths are reported. Verify: Call scope_violations with scope ['[bad'] and a changed file; confirm current empty vs expected error. Remedy: Refuse on empty-after-defaults when --scope was explicitly given, and error (not empty) when any scope glob fails to parse or the set fails to build.
- **F04-6** [minor] `crates/c3/src/consult/detach.rs:1207`, `crates/c3/src/work/record.rs:219` - Consult detached reader death write has no lock or generation check while the work-run equivalent does: two concurrent --status/--wait readers (or a late background done) can race the unsynchronised re-read plus atomic replace. Trigger: Two --status readers judge the same dead run concurrently, or a background writes done between the reader re-read and write_status. Evidence: read-code: ensure_reader_death_record re-reads, checks done/death, builds death, stamps updated and writes; no admission lock and no pid/attempt generation comparison.; read-code: Work path re-reads under admission_lock and requires same_generation plus still-pending plus still-dead before writing died. Verify: Interleave two ensure_reader_death_record calls with a done write between re-read and write; confirm death is not added over done and concurrent writers converge. Remedy: Port the work pattern: re-read under the task admission lock (or an atomic compare) and only add death when the fresh record is the same generation and still not done; never overwrite a background done.
- **F04-7** [minor] `crates/c3/src/consult/orchestrate/mod.rs:2335`, `crates/c3/src/consult/prompt.rs:183` - Grok inline_brief best-effort read silently inlines empty text when the brief file cannot be read, re-creating the invented-brief failure the feature was added to prevent. Trigger: Run grok with a brief path that is missing/unreadable at prompt-build time (race with brief rotation). Evidence: read-code: brief_text is read_to_string(bp).unwrap_or_default() only for inline_brief engines; prompt assembles the <brief> block only when trimmed text is non-empty.; read-code: Empty brief_text means no inlined block is emitted. Verify: Point a grok run at an unreadable brief and check whether the prompt contains a <brief> block or proceeds with path-only reference. Remedy: Fail the grok turn (or refuse to launch) when inline_brief is set but the brief read fails or is empty while a brief was requested; log the read error.

### Prior findings

_(none)_

## Verdict: ADVISE

Advise: binary secret bypass, agy classifier bypass, claude warn-only with hooks, and two fail-open scope/brief paths need fixes before 0.2.4.

### Blockers

_(none)_

### Unproven scenarios

- Byte-for-byte equivalence of retention.rs move (health.rs/quota.rs/verdict.rs call sites and UntilReset/OutWindow boundary semantics).
- Orchestrate split and consult::render→section rename: no missed caller or evaluation-order change (full diff not read).
- Panel re-seat: no duplicate seat/NN, preserved recorded draw order and --panel-seed reproducibility under promotion (full run.rs seat loop not read).
- Redactor coverage for Q1 added-lines (pack/redact.rs not read); binary/large-file/size-cap interactions.
- Staged-outside-repo, submodule, and gitignore-tracked-file paths through finish (provider repo_root resolution not traced).

### First-run checklist (observable)

_(none)_
