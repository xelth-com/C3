# M3 status — `c3 findings` / `c3 scoreboard` / `c3 hook` acceptance walk

Verdicts against `docs/port/m3-acceptance.md`, from the M3 port of `codex-findings.ps1`,
`codex-scoreboard.ps1` and `codex-consult-hook.ps1` (worktree files
`crates/c3/src/{findings_tool,scoreboard,hook}/*`, `crates/c3/src/cli/{findings,scoreboard,hook}.rs`).

Legend: **verified** = exercised and byte-for-byte checked (live parity or a unit test);
**by-construction** = the exact logic/message is ported and unit- or parity-adjacent, but the
specific harness scenario was not run live here; **not yet** = implemented but not exercised
(needs a live child process / seeded fixture a harness would build); **n-a** = out of this
milestone's scope (panel = M4, bridge commit = the consult worker).

Reference scripts: the 0.5.0 plugin cache and repo HEAD (`c4cb428`, wave 24c, one commit past
`50dbddd`) are **byte-identical** for all three scripts (modulo CRLF) and unchanged since
`50dbddd`; `codex-consult-common.ps1` is likewise unchanged over that range. So there is no
cache-vs-HEAD output difference to report.

| harness | check id | verdict | evidence / note |
|---|---|---|---|
| harness-roster BOARD | RATE (mark shape + copy-through) | verified | mutation-parity `findings.json` byte-identical incl. the `ratings` record (keys/order `n,consult_id,lineage,provider,model,purpose,useful,note,when`, lineage/provider/model/purpose copied from the ledger entry). |
| harness-roster BOARD | RATE (re-rate replaces) | verified | `-Rate 3 -Useful partly` re-rated in place (`... re-rated partly (was partly).`), one record kept, byte-identical to the plugin. |
| harness-roster BOARD | RATE (validation) | verified | `-Useful no` w/o `-Note` and `-Useful maybe` refused with the exact wording (live parity + unit test `validate_rating_refusals`); the `-Rate 99` "no consultation n=99 …" message is the same ported string (code path, not run against a 99 fixture). |
| harness-roster BOARD | RATE (legacy entry → unknown provenance) | not yet | `provider=''`, `lineage='unknown provenance'` path is ported; no pre-0.3 no-reviewer entry in the fixtures to run it live. |
| harness-roster BOARD | BOARD (`-Stats` scoreboard) | verified | `-Stats` per-reviewer board byte-identical for `c3-design` and `c3-core-contract` (header + one row per lineage in ledger order, `unknown provenance` last). |
| harness-roster BOARD | RATE (`-List` unaffected) | verified | `--list --all` output byte-identical; it prints no rating text. |
| harness-roster SCORE | SCORE (`-Json` shape, two tasks) | verified | `c3 scoreboard --json` byte-identical to the plugin for both tasks and whole-repo (every numeric field incl. `median_wall_seconds` like `150.14999999999998`, `hit_rate`, tokens). |
| harness-roster SCORE | SCORE (row order) | verified | lineage (case-insensitive, `unknown provenance` last) then purpose; `(total)` after purpose rows; grand `(all)/(total)` last — JSON and table identical. |
| harness-roster SCORE | SCORE (plain table) | verified | table header + every row byte-identical (HIT% `NN%`/`-`, A/H/R/D, Y/P/N, `MEDIAN_S` `-`/rounded, `.NET 0.#` rounding matched incl. the `150.2` edge); `.collab` byte-identical before/after (writes nothing). |
| harness-roster SCORE | SCORE (`-Task` scopes to one task) | verified | `--task <t> --json` / table scoped output byte-identical per task. |
| harness-visibility CONT | CONT (post-timeout-continuation counted usable) | by-construction | scoreboard `usable` uses `c3_core::health::is_usable_outcome` (the two-string rule), the same predicate the acceptance row pins; no CONT fixture run here. |
| harness-engines SCOREBOARD | SCOREBOARD (agy lineage suffix) | by-construction | engine suffix via `format_reviewer_lineage` (`[engine]` for non-codex) — the `[muse]` suffix (`meta :: muse-spark-1.3-contributor [muse]`) is byte-identical in live scoreboard/`-Stats`; no agy/gemini entry in the fixtures. |
| harness-engines SCOREBOARD | SCOREBOARD (`-Stats`/`-Rate` suffixed lineage) | verified | `-Stats` board line `meta :: muse-spark-1.3-contributor [muse]` byte-identical; `-Rate` on the muse consult recorded that exact lineage (mutation parity). |
| harness-fixes F04-1 | F04-1 (empty findings.json refused, `-List`) | by-construction | `read_findings_value` refuses an empty store with the `refusing to use '…': it is empty or could not be read.` family; not run against a 0-byte fixture here. |
| harness-fixes F04-3 | F04-3 (orphan reviewer check surfaced by `-List`) | verified (logic) | orphan-check detection unit-tested (`orphan_check_consults`) and the `[ORPHAN reviewer check: consult N …]` suffix is ported; the real fixtures report `orphans: 0`, so the live suffix was not printed. |
| harness-fixes F04-3 | F04-3 (status change leaves pending untouched) | by-construction | the findings tool never writes/removes a recovery record (only reads them); no `.consult.pending.json` fixture run alongside a status change here. |
| harness-pending (a) | (a) (`-List` shows an interrupted reservation) | not yet | `Write-PendingLine` (`pending: state=…`) ported; no `reserved` recovery record in the fixtures. |
| harness-pending F06-1 | F06-1 (status refused while recorded process alive) | not yet | the recorded-pid liveness rule (`Test-PendingActive`, `pid … is still running`) is ported and uses a real Windows `OpenProcess`/`GetProcessTimes` liveness check, but was not run against a live child fixture. |
| harness-pending F06-1 | F06-1 (unparseable pending refused) | by-construction | `read_pending_file` returns the exact `the recovery record '…' is unusable: …` message; not run against a truncated fixture here. |
| harness-lock2 | LOCK2 (findings refused while `.consult.lock` held) | verified | held the lock with a live pid; `c3 findings --id … --status …` refused **byte-identically** to the plugin (`… is held open by pid <N> on <host> since …`), exit 1. |
| harness-lock2 | LOCK2 (`-List` works under the same lock) | verified | `c3 findings --list` succeeded (exit 0) while the lock was held (it takes no lock). |
| harness-panel RUN | RUN (`-Rate` goes through the store commit) | by-construction | `-Rate` takes `take_write_lock` and rewrites `findings.json` atomically; the guard is dropped (lock freed) before printing. No panel run exercised (M4). |
| harness-panel NOLOSS | NOLOSS (panel leaves no orphans) | n-a | panel is M4; `-List --all` orphan accounting is ported and reads `orphans: 0` on the real stores. |
| harness-panel INFLIGHT | INFLIGHT (status refused during a panel, naming the panel) | by-construction | the task-lock refusal reads the lock's `panel` field and appends `(review panel <shortid>)`; not run with a panel-held lock. |
| harness-panel TIMEOUT | TIMEOUT (`-Rate` refused while a survivor lives) | not yet | recorded-pid rule ported (`a previous consultation's codex process (pid <N>) is still running`); not run with a live survivor. |
| harness-panel BLOCKED | BLOCKED (write lock contended) | by-construction | `write_lock_refusal` emits `^codex-findings: the write lock '…' of task '…' was not acquired within <t> s: …; nothing was changed\.$`; not run against a 60 s-held write lock. |
| harness-panel ORPHAN | ORPHAN (commit killed between findings.json and sessions.json) | n-a | this is the consultation bridge's commit path (the consult worker / `c3_core::store::commit`), not the findings tool. |

## Verification summary (this run)

- **Build / test / lint**: `cargo build` and `cargo test` green (45 tests in `c3`, incl. 9 new
  unit tests: findings validation ×3, `format_finding_line`, orphan/`comparable_path`,
  `finding_consult`, scoreboard median+aggregation, hook ×2); `cargo fmt --check` clean.
  `cargo clippy --all-targets -- -D warnings` is clean **for the M3 files**; it currently
  fails only inside `crates/c3/src/consult/*` (the parallel consult worker's in-progress
  code), not in any file this task owns.
- **Read-only parity (real ledgers, `c3-design` + `c3-core-contract`)**: `--list`, `--stats`,
  `c3 scoreboard` (table and `--json`, per-task and whole-repo) and `c3 hook` are all
  byte-identical to the plugin (CRLF-normalised); `.collab` is byte-identical before/after.
- **Mutation parity (copies)**: the `verified → reopen → rejected(no note, refused) →
  rate(no note, refused) → rate partly` sequence produced a `findings.json` byte-identical to
  the plugin's (only `when` timestamps differ) and identical console output, incl. the history
  record's `base_commit` and `tree_sha256` (`tree sha256 64eed6f52c9b`).
- **Lock refusal**: with `.consult.lock` held by a live pid, `c3 findings --id … --status …`
  is refused byte-identically to the plugin; `--list` still works.
