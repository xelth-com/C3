# Handoff 11 - Meta Muse (muse): merge-framing-meta

Date: 2026-09-26 17:47 local. Author: Meta Muse (muse) (model muse-spark-1.3-contributor, effort high), muse-cli 1.4.0-R4161.1.
Reviewer: meta :: muse-spark-1.3-contributor [muse] (provider from roster, model from roster; engine muse (C:\Users\Dmytro\AppData\Local\Programs\muse\muse.cmd); provider fingerprint 1c6f62bb040d; harness muse-cli 1.4.0-R4161.1).
Preflight: ok: signed in (~/.config/muse/auth.json: providers.meta, mechanism oauth).
Roster: C:/Users/Dmytro/AppData/Local/Temp/claude/C--Users-Dmytro-C3/bada5a2a-1f73-431a-9181-cdc1eb2b5914/scratchpad/roster-no-astra.json - position 10 of 10, panel 043d5bfe member 10 of 10.
Effort: high sent (requested high, mapping muse-v1, by caps-v1: engine:muse, muse-spark-1.3-contributor; not confirmed by the provider). Consultation id: 64be0e3e-142e-4bc0-b112-6f4d9283f035.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only (requested; muse --disable-write --disable-shell --disable-web-tools --approval-mode never; checked by evidence for tracked and untracked files and the collab directory, not for gitignored paths, submodules, files outside the repository or what the reviewer reads), purpose: framing). Argv: `muse exec --json --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-1b590b73e6b242aab787f302efa18dc7.txt --output-schema C:\Users\Dmytro\.claude\plugins\cache\claude-codex-consult\codex-consult\0.5.0\schemas\consult-reply.schema.json --model muse-spark-1.3-contributor --reasoning-effort high --no-foreign-personal-context --disable-web-tools --disable-write --disable-shell --approval-mode never` (prompt from a file: --prompt-file).
Parent thread: (none - new thread). Result thread: `01a0de66-3f8a-7101-afa8-5381033f3a36` (source: events).
Brief: `.collab/c3-design/handoffs/01-claude-merge-framing.md` (sha256 9e75fae84b8b). Reviewed: b13aff1 + uncommitted, base b13aff13385ee8e395a8a9063dc09f1546e747f7, tree sha256 29b74d342082, 9 changed files.
Bridge outcome: usable reply. Wall time: 103.6 s. Tokens: not reported by muse.
Engine turns: 1 (each one a Muse Code subscription prompt); MSP schema_version 1.
Warnings: muse: warning: rules file at \\?\C:\Users\Dmytro\C3\AGENTS.md exists, but the workspace is untrusted, so it is skipped for this session; restart with --trust-workspace to load project rules.
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - Framing is directionally sound but consent, rebuildability, routing sample size, key handling, redaction and locking must be decided before traits freeze. Findings: 1 blocker, 5 major, 0 minor, 0 note (F11-1..F11-6, tracked in `findings.json`). Structured reply: `handoffs/11-muse-merge-framing-meta.reply.json`.
Raw event stream: `handoffs/11-muse-merge-framing-meta.events.jsonl`.
Verbatim reply follows.

---

## Framing challenge

The brief merges three different trust models and calls it one shape: a **read-only evidence bridge** (files, no keys), a **paste-a-slice explainer** (eckSnapshot), and a **ranking retriever + paid panel** (xelth.rs). Those pull in opposite directions: determinism vs probabilistic routing, committable files vs scored memory, sandbox-enforced read-only vs C3-enforced read-only. Freeze `Store`/`Engine` traits before settling that and the traits will encode the wrong invariants.

Unlisted options: (a) **sidecar, not merge** — C3-light stays files-only, SurrealDB/retriever ships as an optional indexer the `http` engine calls; (b) **event-sourced files** — routing state (marks, weights, seed, brief embeddings) as versioned files, DB a disposable cache; (c) **no judge by default** — coordinator merges, judge opt-in per purpose; (d) **snapshot-as-protocol, not artifact** — adopt anchor/delta/redaction/depth rules without the `.eck/` manifest sprawl; (e) **depend via file contract** on xelth brain (`:4446`) behind a trait, port only if it goes stale.

To choose I would need: sample counts for routing (how many rated consults exist?), who provides embeddings offline, binary-size tolerance for `surrealdb`, and whether R17 is opt-in or on-by-default (see finding #1).

## Q1 — files vs DB

Endorse files=record, DB=rebuildable-cache — but only if routing memory is also filed (finding #2). DB-as-truth breaks `.collab` compatibility, makes corruption fatal, and splits locks across file+DB. Keep byte-compatible `.collab/<task>/` writers as the only durable path; DB rebuild must be tested by deletion.

## Q2 — participation policy

Neither option is sound at n=tens. Thompson over thin Beta slices overfits noise; raw softmax over hit-rate inherits HIT%'s verified/(verified+rejected) sparsity (see `.collab/c3-design/context/codex-consult.md`). Prefer pooled hierarchical prior + epsilon-greedy first, `-Topic` as explicit override over BM25/HNSW similarity (cold start), Thompson only past a sample gate. Auditability: ledger must record candidate set, priors, probabilities, seed, draw, and similarity hits — not just the winners.

## Q3 — thickness

Named levels with numeric escapes: `shadow` (1) / `council` (2-3) / `consilium` (all) with per-purpose defaults from R14, plus `--panel k` and `--budget tokens` overrides. Budget alone is unpredictable across labs; k alone ignores cost. Record resolved k, budget, and purpose default in the ledger.

## Q4 — API path

Context-pack-only first. One deterministic budgeted slice + brief + same reply schema preserves read-only without building a tool sandbox. A tool loop reintroduces parity cost, token blowup, and C3-enforced (not sandbox) read-only — make it opt-in later with an allow-list (`read_file`/`grep`) and transcript logging.

## Q5 — port vs depend

Port schema + retriever idioms + `redact()` + T-hub client verbatim (`.collab/c3-design/context/xelth-rs.md`); do not depend on brain `:4446` (mid-refactor). Judge becomes optional `reconcile` stage run by the coordinator's model, never told which seat is its own (avoids self-preference). Copy `acd.rs`/`context.rs` budgeting.

## Q6 — explainer pack

Header must contain: role + answer shape, the verbatim claim, non-goals (no generic review), focus scope, manifest digest, how to cite file markers, closing instruction (mirrors eckSnapshot trailing-prompt lesson). Embed focus files full-text at depth/skeleton within budget; add graph relationship sections only as pointers, not second copies — else the pack exceeds chat windows.

## Q7 — crates and features

Fewer crates: `c3-core`, `c3-store` (with `heavy` feature for `surrealdb`), `c3-engines`, `c3-snapshot+context`, `c3-panel+telemetry`, `c3-mcp`, `c3-cli`. Yes to `heavy` feature — embedded SurrealKV + HNSW drags build size/time; light path must build without it. Fix trait signatures in step 1, not implementations.

## Q8 — missed expensive items

Lineage for `http` seats (`provider::model::endpoint`, no cross-lineage resume); lock ordering file-then-DB + stale recovery (finding #6); R17 consent; R13 `$CODEX_CONSULT_ROOT` + self-review guard + coordinator recorded; path canonicalization (case, separators, `~`, UNC) and atomic write order from step 3; embedding provider for retrieval when offline.

---

### Findings

- **F11-1** [blocker] `README.md:42`, `.collab/c3-design/context/codex-consult.md:46` - R17 telemetry consent is contradictory: plugin decided opt-in on 2026-09-26 while C3 README states ON-by-default. Shipping either without an explicit decision breaks trust and blocks acceptance. Trigger: First run with default env (no CODEX_CONSULT_TELEMETRY set) sends or withholds the per-consultation event. Evidence: read-code: C3 README declares telemetry ON by default with one-line off switch and first-run notice.; read-code: Digest records plugin R17 as decided opt-in on 2026-09-26 and flags the contradiction with C3 README as open. Verify: Read README telemetry section against plugin ROADMAP R17 note and observe first-run/dry-run notice behavior. Remedy: Decide once, in writing, before Store/Engine traits freeze: either reconcile to opt-in everywhere or document why C3 diverges; gate first telemetry send on that decision.
- **F11-2** [major] `.collab/c3-design/handoffs/01-claude-merge-framing.md:63` - "DB rebuildable, losing it loses no evidence" is false for the proposed memory: participation priors, reviewer history and graph edges derived from past briefs live only in SurrealDB and are not reconstructible from files+repo alone. Trigger: Delete SurrealKV directory and re-run selection or retrieval on an existing task. Evidence: read-code: Proposed shape point 2 says DB is index+memory (scoreboard, priors, code graph, cross-project history) yet rebuildable from files+repo.; read-code: Plugin scoreboard today feeds nothing back; R15 plans weighted draw from usefulness+topic, i.e. new state with no file format defined. Verify: Delete the DB dir on a fixture task, rebuild, and diff subsequent panel draw and retrieve_context output vs pre-delete. Remedy: Define the rebuild contract explicitly: either persist routing state (marks, probabilities, seed, brief embeddings) as files and treat DB as pure cache, or admit DB holds durable memory and specify backup/export/corruption recovery.
- **F11-3** [major] `.collab/c3-design/handoffs/01-claude-merge-framing.md:66` - Thompson sampling over per-(member,purpose,topic) Beta posteriors is unsound at tens of consultations: posteriors stay prior-dominated, similarity-weighted slicing makes samples thinner, and the draw will look arbitrary while claiming to be telemetry-driven. Trigger: Early life (fewer than ~50 rated consultations) or a novel topic with no similar stored briefs. Evidence: read-code: Brief proposes Beta posterior + Thompson sampling with epsilon floor and neutral prior, similarity via BM25/HNSW over stored briefs.; read-code: Scoreboard has only CONSULTS/USABLE/HIT%=verified/(verified+rejected) with no aggregate usefulness formula; R15 has zero code. Verify: Simulate draws on a tens-of-rows scoreboard fixture; check selection entropy and sensitivity to one Y/P/N mark. Remedy: Start with epsilon-greedy/softmax over a pooled hierarchical prior (global -> purpose -> topic override), log full posteriors; promote to Thompson only after a stated sample-size gate.
- **F11-4** [major] `.collab/c3-design/handoffs/01-claude-merge-framing.md:71` - Adding the OpenRouter http engine breaks the bridge's core invariant 'never handles a key': an env-var key plus error strings, spool, ledger and judge inputs become new exfiltration surfaces, and redact() coverage is load-bearing. Trigger: Any failed http consultation (401/timeout/retry) that surfaces provider errors, usage payloads, or spooled events. Evidence: read-code: Proposed API path keeps key as env var, never prints/stores it, redact() on every error string.; read-code: Plugin invariant: bridge never creates/prints/pastes a key; Muse refuses when META/MODEL_API_KEY set; xelth redact() strips key and sk-or-shaped tokens (unit-tested). Verify: Fail an http seat with a poisoned error string containing a fake sk-or key; inspect reply file, ledger, spool and stderr for leaks. Remedy: Treat http as a trust boundary: key only via env/process inheritance, deny-list key-shaped strings in ledger/spool/errors with tests, document subscription-vs-API billing guard parity.
- **F11-5** [major] `.collab/c3-design/handoffs/01-claude-merge-framing.md:55` - Porting the eckSnapshot shape verbatim ports its redaction gap: redaction exists only in createSnapshot's output path, not the shared builder, and telemetry posts AnswerToSA-derived content to a hardcoded endpoint. Trigger: Delta snapshot or explainer pack over a tree where a secret appears only in a changed file. Evidence: read-code: Digest notes redaction only in createSnapshot output path, skeletonizer depth-6 JSDoc bug, hardcoded telemetry endpoint with leak risk.; read-code: Proposed c3-snapshot lists discovery/filters/redaction/depth but does not say redaction is centralized and tested on every output path including delta and explainer pack. Verify: Add a secret to one file after the anchor, render delta and explainer outputs, and grep for the raw secret. Remedy: Put redaction as a mandatory last pass in the shared builder used by full, delta, explainer and http context paths, with fixture tests including secrets introduced only in deltas.
- **F11-6** [major] `.collab/c3-design/handoffs/01-claude-merge-framing.md:77` - Lock model is underspecified once a DB exists: today the task lock plus detached status file serialize consultations, but embedded SurrealKV adds a second lock domain (index writes, concurrent panel members, two tools on one repo) with Windows path behavior unaddressed. Trigger: Parallel panel members indexing while a second tool (plugin or C3) opens the same task/DB. Evidence: read-code: R12 decisions: foreground writes starting, background holds .consult.lock, second consult on task blocks; no daemon/queue.; read-code: Q8 names lock model with DB present and Windows/macOS/Linux paths as expensive-to-change but proposal fixes Store/Engine traits in step 1 without fixing it. Verify: Run two concurrent consultations on one task fixture and kill one mid-index; check lock release, status file state, and DB open after restart. Remedy: Specify before step 1: single-writer rule for the DB, lock ordering (file lock then DB txn), stale-lock recovery, and path canonicalization per OS.

### Prior findings

_(none)_

## Verdict: ADVISE

Framing is directionally sound but consent, rebuildability, routing sample size, key handling, redaction and locking must be decided before traits freeze.

### Blockers

- **F11-1** `README.md:42`, `.collab/c3-design/context/codex-consult.md:46` - R17 telemetry consent is contradictory: plugin decided opt-in on 2026-09-26 while C3 README states ON-by-default. Shipping either without an explicit decision breaks trust and blocks acceptance. Verify: Read README telemetry section against plugin ROADMAP R17 note and observe first-run/dry-run notice behavior. Remedy: Decide once, in writing, before Store/Engine traits freeze: either reconcile to opt-in everywhere or document why C3 diverges; gate first telemetry send on that decision.

### Unproven scenarios

- Whether embedded SurrealDB+HNSW binary size and build time are acceptable for the light PowerShell-plugin-adjacent install base.
- Who provides embeddings for retrieval (cloud Gemini vs local Candle/Jina) when the user is offline or keyless, and its cost/latency.
- Whether deterministic budgeted context assembly stays within chat-model windows for large repos without skeleton/depth loss of fidelity.

### First-run checklist (observable)

_(none)_
