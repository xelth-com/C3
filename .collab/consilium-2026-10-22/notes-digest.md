# Consilium digest — remedy-proposal panel (2026-10-22)

Brief: `handoffs/01-claude-remedy-proposals.md`. 10 usable replies, all **ADVISE** (framing consult).
Finder = `openai :: gpt-6-astra` (handoff 12), who gave acceptance criteria, not solutions.

## 1. Members

| Member (provider :: model) | Verdict | Stance on the proposal | Words |
|---|---|---|---|
| meta :: muse-wonder-1.0 (03) | ADVISE | Conditional — pilot on *reopened* findings only | 2942 |
| xiaomi :: mimo-v2.6-pro (04) | ADVISE | Conditional — it's a verification gap, add closing-test duty | 2692 |
| z.ai :: glm-5.3-flash (06) | ADVISE | Conditional — gate on a shipped test, batch blockers | 2945 |
| moonshot :: kimi-k3-256k (07) | ADVISE | Conditional — test-first default, panel only on repeat | 2836 |
| bytedance :: dola-seed-2-0-pro (08) | ADVISE | For — separation sound, wants 3rd-party verifier too | 1747 |
| google :: gemini-3.8-flash-high (10) | ADVISE | Against — 3-role pipeline brittle; use spec contracts | 1873 |
| meta :: muse-spark-1.3-contributor (11) | ADVISE | Conditional — narrow single-solver spike, not a panel | 2066 |
| openai :: gpt-6-astra (12, FINDER) | ADVISE | Conditional-against — pilot before mandatory panel/routing | 2555 |
| z.ai :: glm-5.3 (13) | ADVISE | Conditional — criteria-first; panel is escalation | 3816 |
| deepseek :: deepseek-v4-1-flash (14) | ADVISE | Against (as framed) — right idea, wrong diagnosis | 5035 |

## 2. Per question

**Q1 — finder/solver/verifier split + who verifies.**
*Consensus (9 of 10):* the separation is sound in principle, but the **verifier must not be a third
model by default**. The primary verifier is the **solver's shipped test** (failing before the fix,
green after), re-run mechanically by the coordinator, with the **finder re-checking only its own
trigger on the next pass**. So hold `z.ai :: glm-5.3-flash`, `moonshot :: kimi-k3-256k`,
`meta :: muse-wonder-1.0`, `xiaomi :: mimo-v2.6-pro`, `z.ai :: glm-5.3`, `google :: gemini-3.8-flash-high`,
`meta :: muse-spark-1.3-contributor`, `deepseek :: deepseek-v4-1-flash`, and the finder `openai :: gpt-6-astra`
("solver-authored tests are necessary evidence, not an independent verdict").
*Dissent:* `bytedance :: dola-seed-2-0-pro` alone wants a neutral third model to verify *in addition to*
the test. `google :: gemini-3.8-flash-high` dissents the other way — strict 3-role separation is
"brittle", verification should be mostly automated test execution.
*Keepers:* `deepseek :: deepseek-v4-1-flash` — the finder should be a **challenger** (mandate: find what
the test misses), never a sign-off, and its `verification` must be **frozen before proposals land**;
`meta :: muse-wonder-1.0` — exclude the finder **instance**, not its lab; `xiaomi :: mimo-v2.6-pro` and
`deepseek` — also exclude the **previous fixer's** vendor.

**Q2 — minimal `proposals[]` record + what must not be automated.**
*Consensus (all):* store `findings[].proposals[]` **verbatim inside the existing `findings.json`**, no
new ledger; fields = `{proposer{provider,model,consult_id}, approach, locations[], patch_sketch,
test{name,drives}, risks, confidence, status: adopted|rejected + reason}`, status settable only by the
coordinator. The brief = finding record + a pack of located code and nearby tests. **Never automate:**
adoption/selection, merging, worker launch, the `verified` flip, and the decision to spend the window
(the `--remedy` line is *offered*, not run).
*Dissent:* none on structure. `bytedance :: dola-seed-2-0-pro` carries the thinnest record (drops
consult_id/base_commit).
*Keepers:* `openai :: gpt-6-astra` — immutable original replies, append-only **decision events**,
base/finding/pack **digests**, *adoption must not change finding status*; `deepseek :: deepseek-v4-1-flash`
— `test.fails_before` and `pack_digest`, an explicit **"no proposal adopted"** outcome, and record the
exclusion + panel composition in `sessions.json`; `z.ai :: glm-5.3` — a separate `findings[].acceptance[]`
the finder writes before seeing proposals.

**Q3 — live remedies.** See section 3.

**Q4 — scoring.**
*Consensus (all):* credit must be **outcome-gated on `verified`, never on adoption** (adoption alone = 0),
with **separate `finder` and `solver` router dimensions** so a good solver who is a mediocre finder is
still routable. Named: every member.
*Dissent:* only `bytedance :: dola-seed-2-0-pro` originally leaned on adoption for routing but still
gates the solver point on verification; `google :: gemini-3.8-flash-high` is the sharpest — adoption
earns **zero** permanent credit, regressions actively penalize (-1.5).
*Keepers:* `deepseek :: deepseek-v4-1-flash` — a **signed ledger (not counters)**, key solver score by
**(purpose, topic)**, and the finder may not rate the solver; `z.ai :: glm-5.3` — defer the router's
solver dimension until ~10 verified samples; `openai :: gpt-6-astra` — use shrinkage/priors + exploration
and compare **closure time and reopen rate**, not just correctness.

## 3. Remedies for F91-1..F91-4

Astra (`openai :: gpt-6-astra`) is the finder; it independently analyzed **only F91-1 and F91-2**
(F91-3/F91-4 deferred — yardstick there is handoff 91's own RC2/RC3).

**F91-1 (blocker — stale resume admission).** *Proposed by all nine solvers; near-identical designs.*
Design: make `claim_for_spawn` the single authority — pass it the resumable-status set / expected
generation instead of a precomputed attempt; **under `admission_lock`** reread the record, refuse if
terminal / quota-exhausted / foreign-live / generation-mismatched, derive `n = quota.resumes+1` from
that same locked read, and write `running` atomically. `resume_once` (mod.rs:1142) stops precomputing
`n`; `do_resume_foreground` (mod.rs:1386) keeps its checks only as fast-fail UX.
Functions: `claim_for_spawn` (mod.rs:747), `resume_once`, `do_resume_foreground`.
Tests: `resume_claim_rejects_stale_snapshot` (astra) / `resume_refuses_stale_claim_after_completion`
(mimo) / `claim_for_spawn_rejects_terminal_and_exhausted` (muse-wonder) / `work_resume_stale_claim_refused`
(muse-spark) / barrier variants (glm, kimi, gemini, deepseek `stale_resume_claim_is_refused`).
**Astra's acceptance criteria (yardstick):** reject delayed claims after `done`, after exhausted quota,
*and* after a later resumable generation, with **no second launch marker**; a valid alarm claim must
still succeed; permit `waiting_quota` **only for its matching alarm owner**; and split pure record
reading from recovery mutation so a stale recovery writer can't undo a fresh claim.
*Agreement:* all nine agree on the core (validate+derive under the lock). Only `deepseek` and
`z.ai :: glm-5.3` additionally flag the `waiting_quota`/alarm-owner path and the `read_record`
recovery-write problem that Astra's criteria require — the others would break the alarm resume if
implemented naively.

**F91-2 (blocker — unreadable record treated as absent).** *Proposed by muse-wonder, glm-5.3-flash,
kimi, gemini, muse-spark, glm-5.3, deepseek.* Design: `read_record` returns `Ok(None)` **only** on
`ErrorKind::NotFound`, `Err` otherwise; `run_dirs`/`live_run`/`other_live_run` return `Result` and
propagate enumeration/entry/parse errors; admission (both `run()` start path and `claim_for_spawn`)
**refuses on any uninspectable candidate**. Fail closed.
Functions: `read_record` (record.rs:137), `run_dirs` (187), `live_run`/`other_live_run` (211/223),
callers in `mod.rs` (~749, 904).
Tests: `admission_refuses_when_candidate_uninspectable` / `admission_refuses_on_unreadable_record` /
`unreadable_record_refuses_admission` — live worker + denied read on its `run.json`, second start
refuses before launch.
**Astra's acceptance criteria:** refuse before allocation/launch for **read, metadata, enumeration,
entry and parse** failures; **replace `is_file()` with fallible metadata inspection**; keep absent-root
and readable-terminal-record controls behaving as before.
*Agreement:* all seven agree on the fail-closed core. `deepseek :: deepseek-v4-1-flash` uniquely proposes
a **strict/lenient split** (`read_record_strict` for admission, keep lenient `read_record` for display
callers) to bound the blast radius; `z.ai :: glm-5.3` uniquely insists the pack/patch update the caller
`run()` at mod.rs:904.

**F91-3 (major — destructive artifact reads).** *Proposed by mimo, glm-5.3-flash, kimi, dola-seed,
glm-5.3, deepseek. Astra did not analyze it.* Design: replace `unwrap_or_default` on attempt and
aggregate event reads with propagated errors routed to `finish_artifact_error`; delete temp attempt
files **only after** the bytes are read and the append durably succeeds; `refresh_artifacts` must not
rewrite `report.md` from an unreadable source.
Functions: `refresh_artifacts` (mod.rs:858), `resume_once`/`append_resume` (1175-1192).
Tests: `resume_event_read_failure_keeps_attempt_bytes` (mimo) / `artifact_read_failure_preserves_bytes`
(glm/kimi/deepseek) / `read_failure_preservation` (dola-seed).
**Yardstick (handoff 91 RC2):** injected read failures on attempt and aggregate streams → preserved
source bytes, `artifact_error`, nonzero exit, old `report.md` intact.
*Agreement:* all six agree; `deepseek :: deepseek-v4-1-flash` flags it as a **singular ordering fix —
do not send to a design panel**.

**F91-4 (major — weak terminal-success detector).** *Proposed by muse-wonder, mimo, glm-5.3-flash,
kimi, glm-5.3, deepseek. Astra did not analyze it.* Design: Claude success only when the result's
`is_error` is explicitly `Some(false)` (absent → `Terminal::None`); Codex success only from
`turn.completed` — **delete the `agent_message` arm** from the success detector but keep `agent_message`
feeding report extraction in `summarize`. `classify_attempt` already fails closed on `Terminal::None`.
Functions: `terminal_outcome` (engines.rs ~480/504), `summarize_events` (record.rs:544); invert the unit
test at engines.rs:642/651 that currently enshrines the bug.
Tests: `terminal_outcome_requires_affirmative_success` (mimo) / `terminal_requires_affirmative_success`
(kimi) / `exit_zero_without_affirmative_terminal_is_not_done` (glm-5.3) / `claude_bare_result_is_died` +
`codex_agent_message_without_turn_completed_is_died` (glm-5.3-flash).
**Yardstick (handoff 91 RC3):** exit-zero fake launchers emitting a bare Claude `result` or a Codex
`agent_message`-only stream must classify `died`, never `done`.
*Agreement:* all six are essentially identical, incl. flipping the codified test.

## 4. Watch-outs (must-not-automate / risks)

- **Vendor diversity ≠ independence.** `deepseek :: deepseek-v4-1-flash`, `meta :: muse-spark-1.3-contributor`,
  `meta :: muse-wonder-1.0`: shared training data + a shared brief give correlated proposals.
  `deepseek`: `--exclude-vendor` can leave 0–1 eligible solvers — **require ≥2 after exclusion or skip
  and record the skip** in `sessions.json`; measure divergence (replay to 2 labs) before building it.
- **Pack-fed solvers miss call sites.** `z.ai :: glm-5.3`, `google :: gemini-3.8-flash-high`,
  `meta :: muse-wonder-1.0`: a signature change (F91-2) must also update `run()` at mod.rs:904.
  `z.ai :: glm-5.3` — **prefer repo-reading engines (claude, codex) as solvers** or fix the pack template.
- **`read_record` is not a pure reader** — it performs recovery writes; `openai :: gpt-6-astra` requires
  splitting read from recovery mutation and serializing recovery with admission. A naive
  "foreground-only allowed-state" rule **breaks the alarm `waiting_quota` resume path** (Astra).
- **Window %, not wall-clock, is the scarce resource** — `deepseek`: N solvers cost N×280 s of window;
  the panel does **not** save the 40–60 min worker wave. Budget per finding *family*, cap at 2 solvers,
  pre-commit the window.
- **A panel always yields a fix and launders the decision** unless an explicit "no proposal adopted"
  outcome exists (`deepseek`).
- **Do not automate** (consensus, esp. `google :: gemini-3.8-flash-high`): panel auto-dispatch on HOLD
  (token exhaustion), proposal adoption/merging, finding closure without a test pass, the `verified`
  flip, vendor-exclusion policy, and the finder amending acceptance criteria after proposals exist.
- **Adoption must not feed the router as skill, nor change finding status** (`openai :: gpt-6-astra`,
  `z.ai :: glm-5.3`).
- **F91-3 is singular-design** — not a panel candidate (`deepseek`).

## 5. Scoring ideas (Q4)

- Credit the solver **only on `verified`** (adopted + test green + finder closes next pass); adoption
  alone = 0 (consensus).
- Keep **separate `finder` and `solver` router dimensions** (consensus).
- **Signed ledger, not counters**: reverse solver credit when a later finding with the same trigger
  supersedes it (`z.ai :: glm-5.3-flash`, `moonshot :: kimi-k3-256k`, `z.ai :: glm-5.3`, `deepseek`).
- **Denominator** counts rejected/duplicate findings against the finder and rejected proposals against
  the solver, so volume dilutes (`meta :: muse-wonder-1.0`, `xiaomi :: mimo-v2.6-pro`, `moonshot :: kimi-k3-256k`).
- **Supersedes-of-own-finding discount** — a self-split earns its delta once
  (`muse-wonder`, `mimo`, `glm-5.3`, `deepseek`).
- **Key solver score by (purpose, topic)**, not globally (`deepseek :: deepseek-v4-1-flash`).
- **Minimum sample before solver routing bites**: ≥2 verified (`meta :: muse-spark-1.3-contributor`)
  or ~10 verified (`z.ai :: glm-5.3`).
- **Orthogonal dimensions** `finder_precision`, `spec_clarity`, `solver_verified_efficacy`; +1.0 verified,
  −1.5 regression (`google :: gemini-3.8-flash-high`).
- **Finder may not rate the solver**; the `useful` rating is the coordinator's (`deepseek`, `xiaomi :: mimo`).
- **Partial credit** when a proposal is merged/reworked (`muse-wonder`, `muse-spark`, `openai :: gpt-6-astra`).
- Use **shrinkage/priors + exploration**; evaluate **closure time and reopen rate** alongside
  correctness (`openai :: gpt-6-astra`).
