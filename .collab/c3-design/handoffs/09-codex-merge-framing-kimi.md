# Handoff 09 - Codex: merge-framing-kimi

Date: 2026-09-26 17:47 local. Author: Codex (model k3, effort high), Codex CLI 0.155.1.
Reviewer: kimi :: k3 (provider from roster, model from roster; endpoint https://api.kimi.ai/coding/v1, wire_api: responses; provider fingerprint 8f7901d404b5; harness codex-cli 0.155.1).
Preflight: ok: env KIMI_API_KEY set.
Roster: C:/Users/Dmytro/AppData/Local/Temp/claude/C--Users-Dmytro-C3/bada5a2a-1f73-431a-9181-cdc1eb2b5914/scratchpad/roster-no-astra.json - position 8 of 10, panel 043d5bfe member 8 of 10.
Effort: high sent (requested high, mapping kimi-v1, by caps-v1: api.kimi.ai, k3; not confirmed by the provider). Consultation id: 255079da-0d82-4a45-bd6c-2b027a669fda.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `codex exec --sandbox read-only --color never --json -m k3 -c model_reasoning_effort="high" -c model_provider="kimi" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-05dd609ba2f044d78a543e934b026cf0.md -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a0de66-2d23-7880-a2e3-4ed63847f8df` (source: events).
Brief: `.collab/c3-design/handoffs/01-claude-merge-framing.md` (sha256 9e75fae84b8b). Reviewed: b13aff1 + uncommitted, base b13aff13385ee8e395a8a9063dc09f1546e747f7, tree sha256 29b74d342082, 9 changed files.
Bridge outcome: usable reply. Wall time: 277 s. Tokens: in 278689 (cached 177664), out 5687, reasoning 1343.
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - The proposed six-point shape is broadly sound, but the framing hides a deferrable-DB option, and four concrete risks (R17 contradiction, rebuild-less dual store, unproven similarity priors, lock model) should be resolved before the Store/Engine traits are frozen. Findings: 0 blocker, 3 major, 2 minor, 1 note (F09-1..F09-6, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/09-codex-merge-framing-kimi.reply.json`.
Raw event stream: `handoffs/09-codex-merge-framing-kimi.events.jsonl`.
Verbatim reply follows.

---

## Framing challenge first

The brief presents "files = record, DB = rebuildable index" as the baseline and "DB as source of truth" as the only serious alternative. A third option is not listed: **defer SurrealDB entirely**. Ship the pure port (steps 1–4 of the README port order) with `Store = files` only, run the participation policy over the JSON scoreboard (Thompson sampling needs only per-(reviewer, purpose) counts — trivially computable from `sessions.json`/`findings.json` at tens of consultations), and add `surreal` later behind the trait. The DB's real justification is narrow: instant context assembly for `http` reviewers and cross-project memory. Neither exists in the port's critical path. Every answer below changes if you take this option: Q1 becomes moot until step 5+, Q2 loses its BM25/HNSW dependency, Q7's feature-flag question disappears.

To choose, I would need to know: (a) the expected corpus size and consultation volume in the first six months — if it's tens of consultations on a handful of repos, BM25-over-briefs is statistically empty anyway; (b) whether `http` reviewers are a launch requirement or a later audience; (c) whether the maintainer is willing to run a DB-rebuild check in CI as proof of the "rebuildable" claim. If (a) is small, take the defer option; the trait seam costs almost nothing if `Store`/`Engine` are fixed in step 1 as proposed.

## Q1. Files = record, DB = rebuildable index

Right split — keep it. DB-as-truth breaks committable evidence, plugin compatibility, and recovery (alternative 1's own analysis is correct). But the split as stated has a hole: nothing defines the **write order, staleness detection, or rebuild command**. Rule: files are written first and fsync'd (the plugin's atomic write order, port step 3), the DB indexes them after, and every DB row carries the hash of the file it came from; `c3 doctor --rebuild` must exist before the DB feeds any decision. Locks: the existing `.consult.lock` (R12) governs consultations; SurrealKV's own single-writer file lock is a *second* lock that will conflict with detached parallel panels — see finding #4. Two tools on one repo (plugin + c3) both appending to `sessions.json` is fine only if the append protocol stays byte-compatible; make that an eval.

## Q2. Participation policy

Thompson sampling over Beta posteriors is sound at tens of consultations — that is precisely the small-sample regime it is for, and softmax-over-hit-rate is strictly worse there (HIT% is null when verified+rejected = 0, per the digest's scoreboard definition). What is **not** sound at this scale is deriving the prior from BM25/HNSW similarity over stored briefs: with tens of briefs the similarity surface is noise, and pseudo-counts weighted by it will dominate the real marks. Start with per-(reviewer, purpose) posteriors plus explicit `-Topic` tag override (R15 as already decided); add similarity smoothing later when the corpus justifies it. Auditability: record seed, posterior parameters, epsilon, and the draw in the ledger (already in the brief — keep it), and make the draw a pure function `(ledger_state, seed) -> panel` so `c3 replay <session>` reproduces it. Note: no plugin eval exists for R15 — the README's "plugin evals as oracle" guarantee ends here, so write golden tests from decided behavior instead (finding #3).

## Q3. Thickness

Named levels (`shadow`/`council`/`consilium`) with per-purpose defaults in R14's table, implemented as aliases for (k, token budget) pairs; `--panel k` and `--budget` as explicit overrides. Named levels carry the R14 stakes semantics; raw numbers are for the maintainer tuning. Do not ship budget-only — "cheap" reviewers differ by 10x in token cost, so a bare token budget silently re-weights the panel toward cheap labs and fights the `lab` diversity rule.

## Q4. API path

Context-pack-only first, unconditionally. Read-only for `http` reviewers then holds *by construction* (no tools exist to misuse), not by C3 policy enforcement — this preserves the bridge's core trust property. The tool loop (alternative 2) can follow as opt-in, but it converts read-only from a sandbox guarantee into code you must test, and it multiplies cost/latency per reviewer. One requirement: the pack generator must run the same redaction pipeline as snapshots before anything crosses the wire (finding #5).

## Q5. xelth.rs: port vs depend

Port the schema + retriever *patterns* (BM25+HNSW fused by RRF, then multi-hop; the SurrealDB v3 idioms in the digest are the valuable part), not the code as a dependency — alternative 4 correctly rejects depending on a mid-refactor service on :4446. Copy the T-hub client near-verbatim (~35 lines, already the reference). Take the consilium's panel-resolution and `redact()` shape; make the judge an optional `c3 reconcile` stage, but let the coordinator's own model be judge only with the xelth.rs convention that it is told which seat is its own — and consider defaulting the judge to a roster member to avoid self-grading.

## Q6. Explainer pack

Header must contain, in order: the role/answer-style instruction (answer **only** from the included material, say "not in the pack" rather than extrapolating), the claim verbatim, the `.eck` digest, then focus file bodies in full with `--- File: ---` markers, then graph *relationship sections* for non-focus entities (labels only, no bodies — the xelth.rs retriever output shape), then a closing instruction restating the claim. eckSnapshot's README warning is the key evidence: the trailing prompt must be pasted first or the model reverts to generic review — so the pack should open and close with the instruction, not rely on position. Bodies for focus files, relationship labels for everything else: full bodies for the whole graph blow the budget and add nothing.

## Q7. Crates and dependency budget

Workspace as proposed, but merge `c3-telemetry` into `c3-core` (it's ~35 lines and one spool writer; a crate per 35 lines is fragmentation, not boundaries) and watch that `c3-context` depending on `c3-store` doesn't create a cycle when `surreal` wants the retriever. `surrealdb` + `candle`-class deps behind a `heavy` cargo feature, yes — embedded SurrealKV compile times are brutal and the light path (steps 1–4) should build in seconds. Keep `Store` object-safe and `files` always compiled.

## Q8. Expensive-to-change-later gaps

1. **R17 contradiction** (finding #1): plugin decided opt-in 2026-09-26; C3 README says ON by default. Resolve before any telemetry ships — it's a trust property, not a toggle.
2. **Lock model with a DB** (finding #4): `.consult.lock` + SurrealKV single-writer + detached panels is underspecified; pick one writer or a DB lock arbitration now.
3. **`http` lineage semantics** (finding #6): the plugin's continuation/resume rules assume stateful threads; stateless HTTP calls need an explicit mapping (full-history resend vs single-shot), and the ledger must record the context-pack hash as part of lineage.
4. **R13 host invariance vs DB location**: the DB must live under the config dir, never under `$CODEX_CONSULT_ROOT` or the repo, and machine-absolute paths must not leak into committable files (entity ids `path::name` must be repo-relative with normalized separators — Windows/macOS/Linux).
5. **Redaction placement** (finding #5): eckSnapshot redacts only in one output path; C3 must put `redact()`/secret-scan in the shared builder feeding snapshots, explainer packs, and `http` context packs alike.

---

### Findings

- **F09-1** [major] `README.md:42`, `.collab/c3-design/context/codex-consult.md` - Telemetry default is contradictory across the two sources of truth: the plugin ROADMAP decided R17 as opt-in on 2026-09-26, while C3's README ships 'on by default, off with one line'. The brief itself flags this as open but does not resolve it, and the proposed shape (point on telemetry/T-hub) inherits the ambiguity. Trigger: First C3 release with telemetry code: whichever default ships, one of the two documented commitments is violated. Evidence: read-code: Heading 'Telemetry (on by default, off with one line)' with CODEX_CONSULT_TELEMETRY=off as the opt-out.; read-code: R17 entry: 'decided opt-in on 2026-09-26 in the plugin ROADMAP - while C3's README says ON by default... This contradiction is open.'. Verify: Diff the plugin repo ROADMAP R17 entry against README.md:42-60 and record the maintainer's single decision in both places before step 5 of the port order starts. Remedy: Pick one default explicitly (recommend: keep C3's on-by-default only if the plugin is amended to match, with the disclosed-once notice; otherwise opt-in in both), and note the divergence rule the digest already states: C3 should not diverge from plugin decisions without saying so.
- **F09-2** [major] `.collab/c3-design/handoffs/01-claude-merge-framing.md` - The 'SurrealDB is rebuildable from files + repo; losing it loses no evidence' guarantee has no defined rebuild command, no provenance/staleness mechanism, and no write-order rule; once participation priors are drawn from the DB, silent file/DB drift corrupts reviewer selection without any error surfacing. Trigger: A crash or concurrent write between the files append and the DB index update, or any manual edit of findings.json, leaves the DB stale; subsequent Thompson draws use wrong posteriors. Evidence: read-code: States the rebuildable property as an assertion; no rebuild tool, hash check, or write order is specified anywhere in the brief or digests.; inferred: R15 requires the draw to be recorded and the scoreboard to feed selection, so stale priors directly alter behavior. Verify: After Store is implemented, kill c3 between the files write and the DB write, run the rebuild command, and assert the posteriors match a from-scratch index. Remedy: Specify: files written and fsync'd first (plugin atomic write order), DB updated second, every DB row stores the source file hash; ship `c3 doctor --rebuild` before any DB-derived decision (participation, retrieval) goes live, and run rebuild-verification in CI.
- **F09-3** [major] `.collab/c3-design/handoffs/01-claude-merge-framing.md`, `README.md:20` - Deriving Beta-prior pseudo-counts from BM25/HNSW similarity over stored briefs is statistically empty at the stated scale (tens of consultations), and the README's 'plugin evals as the oracle' strategy has no oracle for R15 routing because the plugin implements none of it - this is the first port step with zero specification-by-implementation. Trigger: Early consultations where similarity-weighted pseudo-counts outnumber real Y/P/N marks, producing reviewer selections that look data-driven but reflect embedding noise. Evidence: read-code: 'R15 ... zero code' and scoreboard 'feeds NOTHING back into selection'; HIT% is null when verified+rejected = 0.; read-code: Port order preamble: 'each step lands with tests and the plugin's evals as the oracle'. Verify: Simulate 30 consultations with a fixed reviewer quality ordering under both (a) plain per-(reviewer,purpose) Beta posteriors and (b) similarity-smoothed posteriors; compare panel selection regret. Remedy: Ship R15 exactly as decided (per-(reviewer,purpose) posterior + -Topic override + epsilon + neutral prior); gate similarity smoothing behind a corpus-size threshold; write golden tests from the R15 decision text in place of plugin evals.
- **F09-4** [minor] `.collab/c3-design/handoffs/01-claude-merge-framing.md` - The lock model with an embedded DB is unspecified: R12's .consult.lock serializes consultations per task, but SurrealKV enforces its own single-writer file lock per database, so detached parallel panels (R12) writing index updates to one shared DB will fail or serialize unpredictably. Trigger: Two detached consultations on different tasks in the same repo both finishing and attempting DB index writes concurrently. Evidence: read-code: Background process holds .consult.lock per task; nothing arbitrates a repo-level resource like an embedded DB.; read-code: Embedded SurrealKV, no server - implies process-local single-writer storage. Verify: Run two `c3 consult -Detach` on different tasks of one repo against a shared DB and observe the second writer's behavior. Remedy: Decide now: one DB-writer model (index updates queued to whichever process holds a repo-level DB lock, else skipped and caught by next rebuild) or per-task DB shards merged on read; record the choice next to R12's D1-D12.
- **F09-5** [minor] `.collab/c3-design/context/eckSnapshot.md` - Redaction in eckSnapshot exists only in createSnapshot's output path, not the shared builder; if C3 ports that shape, `http` reviewer context packs and explainer packs - the two outputs that leave the machine - can carry secrets that snapshot creation alone would have stripped. Trigger: A secret matching the scanner's patterns in a file selected by graph retrieval for an http context pack. Evidence: read-code: Rough edges: 'secret redaction only in createSnapshot's output path, not in the shared builder'.; read-code: scanner.rs ported from eckSnapshot; redact() only covers API keys in error strings, not file bodies. Verify: Unit test: a fixture file containing an sk-... token must be redacted identically in snapshot, explainer pack, and http pack outputs. Remedy: Put secret scanning/redaction in c3-snapshot's shared render pipeline (the equivalent of renderFileAtDepth), so every output path inherits it; keep redact() on error strings as a separate layer.
- **F09-6** [note] (no location) - The plugin's continuation/timeout semantics assume stateful CLI threads ('one continuation turn on the same thread; never fork/resume across lineages'); http reviewers are stateless single calls, so lineage, resume, and the ledger's lineage field have no defined meaning for the http engine class. Trigger: A timeout or format-repair on an http reviewer: there is no thread to continue, and a naive retry silently becomes a new consultation with a fresh draw. Evidence: read-code: Timeout and format-repair rules are defined per-thread; lineage = provider+model+endpoint with no forking across lineages.; inferred: 'one call, same reply schema' - no continuation semantics mentioned. Verify: Decide and document the http retry rule, then check that a retried http consultation produces ledger entries that replay deterministically. Remedy: Define http consultations as single-shot: timeout/format-repair resends the full pack as a new ledger entry linked by supersedes-style reference; record the context-pack content hash in lineage so the exact input is auditable.

### Prior findings

_(none)_

## Verdict: ADVISE

The proposed six-point shape is broadly sound, but the framing hides a deferrable-DB option, and four concrete risks (R17 contradiction, rebuild-less dual store, unproven similarity priors, lock model) should be resolved before the Store/Engine traits are frozen.

### Blockers

_(none)_

### Unproven scenarios

- Whether embedded SurrealKV's single-writer behavior under concurrent detached panels fails loudly or blocks silently (finding #4) - inferred from the digest, not tested.
- Whether Thompson sampling vs softmax materially differs on this roster at real consultation counts - argued from small-sample properties, not simulated.
- Whether xelth.rs's retrieve_context latency at C3's expected corpus sizes actually meets the 'instant' requirement - no numbers in the digest.
- Windows/macOS path normalization needs in deterministic entity ids (path::name) - flagged in Q8 but not verified against xelth.rs code.

### First-run checklist (observable)

_(none)_
