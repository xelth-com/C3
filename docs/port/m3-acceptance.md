# M3 acceptance checklist — ledger and findings assertions

Derived from `C:\Users\Dmytro\claude-codex-consult` at commit `023123f`'s ancestor tree (same
checkout used for the M1 checklist). Every row is one or more `Check`/`Say` calls a harness
makes against `codex-findings.ps1` and `codex-scoreboard.ps1` (or the `Findings` helper that
wraps the former), condensed to one row per distinct assertion group rather than one row per
individual `Check` call — the same convention `m1-acceptance.md` uses. "Check id" is the
harness's own tag string (first argument of `Check`/`Say`) plus a short disambiguator where a
tag repeats. File:line is omitted here as in `m1-acceptance.md`'s rows that group several
calls; section names point at the harness's own `# ===` banner or `if (Want '<id>')` block.

C3's `EvidenceStore` (`c3 findings`, `c3 scoreboard`) is M3-complete when every row below
passes unmodified against the corresponding harness section. C3's own contract names are used
throughout: `findings.json`, `sessions.json` (the ledger), and "the ledger entry is the commit
point" (DESIGN.md §4).

| harness | section | check id | one-line expectation | exit code |
|---|---|---|---|---|
| harness-roster.ps1 | BOARD | RATE (mark shape + copy-through) | `-Rate 1 -Useful yes` on a task with 4 ledger entries -> prints `codex-findings: consult n=1 (openai :: gpt-5.1, decision) rated yes.`; `findings.json` gains a top-level `ratings` array with exactly one record whose keys are `n, consult_id, lineage, provider, model, purpose, useful, note, when` (lineage/provider/model/purpose copied from ledger entry n=1); `findings` count (8) unchanged | 0 |
| harness-roster.ps1 | BOARD | RATE (re-rate replaces) | rating the same n again (`-Rate 1 -Useful partly -Note "half of it"`) prints `... re-rated partly (was yes).` and replaces the one record in place (still 1 record, new `useful`/`note`) | 0 |
| harness-roster.ps1 | BOARD | RATE (validation) | `-Useful no` without `-Note` -> `-Useful no needs -Note (why the consultation was not useful).`; `-Rate 99` (no such n in `sessions.json`) -> `no consultation n=99 in <sessions.json path>; -Rate takes the n of a ledger entry (see -Stats).`; `-Useful maybe` -> `-Useful must be yes, partly or no (got 'maybe').` | 1 |
| harness-roster.ps1 | BOARD | RATE (legacy entry -> unknown provenance) | rating a ledger entry with no `reviewer` object -> the mark's `lineage` is `'unknown provenance'`, `provider` is `''` | 0 |
| harness-roster.ps1 | BOARD | BOARD (`-Stats` scoreboard) | `-Stats` prints a `reviewers (findings by the lineage ...)` header line followed by the column header `reviewer raised verified implemented proposed rejected wontfix superseded yes partly no`, then one row per lineage in ledger order (a reviewer that raised nothing still gets a row), `unknown provenance` last (covers both a legacy no-reviewer entry and a finding whose consult has no ledger entry); counts and the judge's yes/partly/no marks per lineage match the seeded findings/ratings exactly | 0 |
| harness-roster.ps1 | BOARD | RATE (`-List` unaffected) | `-List -All` after rating shows no rating text (`-notmatch 'rated|useful|partly'`) — ratings are a separate concern from status | 0 |
| harness-roster.ps1 | SCORE | SCORE (`-Json` shape, two tasks) | `codex-scoreboard.ps1 -Json` over two seeded tasks (alpha, beta) with mixed reviewers/purposes: exactly 9 rows — one per (lineage, purpose), one lineage-total row (`purpose == '(total)'`) per lineage, and one grand-total row (`lineage == '(all)'`, `purpose == '(total)'`); every numeric field (`consults, usable, prose, failed, raised, verified, rejected, wontfix, superseded, open, hit_rate, verdict_accept/hold/reject/advise, rated_yes/partly/no, median_wall_seconds, tokens_in, tokens_out`) matches the hand-computed expectation for each row | 0 |
| harness-roster.ps1 | SCORE | SCORE (row order) | rows sort by lineage (case-insensitive, `unknown provenance` last), then by purpose within a lineage, the lineage's `(total)` row immediately after its purpose rows, the grand `(all)\|(total)` row last | n/a |
| harness-roster.ps1 | SCORE | SCORE (plain table) | no `-Json`: header line matches `^REVIEWER\s+PURPOSE\s+CONSULTS\s+USABLE\s+PROSE\s+FAILED\s+RAISED\s+VERIFIED\s+REJECTED\s+WONTFIX\s+SUPERSEDED\s+OPEN\s+HIT%\s+A/H/R/D\s+Y/P/N\s+MEDIAN_S\s+TOKENS$`; a `(none)`-purpose row and the grand-total row both format correctly (`HIT%` as `NN%` or `-` when verified+rejected==0, `A/H/R/D` and `Y/P/N` as slash-joined counts, `MEDIAN_S` as `-` or a rounded number); running it writes nothing under `.collab` (byte-identical file list/size/mtime before and after) | 0 |
| harness-roster.ps1 | SCORE | SCORE (`-Task` scopes to one task) | `-Task beta -Json`: the sole `kind == 'total'` row reflects only that task's 3 consults / 3 raised findings / ratings 1 yes, 0 no (not the whole `.collab`'s totals) | 0 |
| harness-visibility.ps1 | CONT | CONT (scoreboard counts a post-timeout-continuation reply as usable) | after a run whose bridge_outcome is `'usable reply (after a timeout continuation)'`, `codex-scoreboard.ps1 -Task t -Json`'s grand-total row's `usable` field counts it (matches `Test-UsableOutcome`'s own two-string rule, mirrored in `c3_core::health::is_usable_outcome` per the M1 wave-24b note) | 0 |
| harness-engines.ps1 | SCOREBOARD | SCOREBOARD (agy lineage suffix) | a consultation run through the `agy` engine (gemini) shows in `codex-scoreboard.ps1`'s output as lineage `gemini :: <model> [agy]` (1 consult, 1 raised) while a plain codex-engine lineage is unaffected — `Format-ReviewerLineage` appends `[<engine>]` only when engine != codex | 0 |
| harness-engines.ps1 | SCOREBOARD | SCOREBOARD (findings `-Stats`/`-Rate` use the same suffixed lineage) | `codex-findings -Stats` prints a board line starting `gemini :: <model> [agy]`; `-Rate` on that consultation records `lineage == "gemini :: <model> [agy]"` with the standard rating-record field set | 0 |
| harness-fixes.ps1 | F04-1 | F04-1 (empty findings.json refused, `-List`) | `findings.json` present but empty (0 bytes) -> `-List` is refused with a message matching `refusing` (the same "refusing to use ... empty" family the store layer uses for any empty JSON store) | 1 |
| harness-fixes.ps1 | F04-3 | F04-3 (orphan reviewer check surfaced by `-List`) | a finding whose `reviewer_checks` names a consult number with no ledger entry (e.g. consult 2 when the ledger only has n=1) -> `-List`'s line for that finding is suffixed `[ORPHAN reviewer check: consult 2 ...]` and the summary's `orphans: ... reviewer check(s)` count reflects it | 0 |
| harness-fixes.ps1 | F04-3 | F04-3 (a status change leaves the pending/recovery record untouched) | `codex-findings -Id F02-1 -Status implemented` while an interrupted consultation's `.consult.pending.json` exists -> that file is byte-identical (same SHA-256) before and after, and the status change itself succeeds (exit 0) — the pending record is read only, never modified, by a findings status change | 0 |
| harness-pending.ps1 | (a) | (a) (`-List` shows an interrupted reservation) | a `reserved`-state pending record (no live process) -> `-List`'s output contains a `pending: state=reserved, n=<N>, nn=<NN>` line describing it | 0 |
| harness-pending.ps1 | F06-1 | F06-1 (status change refused while the recorded process is alive) | a `running`-state pending record naming a live child pid -> both a new consultation run and `codex-findings -Id ... -Status implemented` are refused, each mentioning `(pid <N>) is still running`; the pending record stays byte-identical across both refusals | 1 |
| harness-pending.ps1 | F06-1 | F06-1 (status change refused on an unparseable pending record) | `.consult.pending.json` truncated/corrupt (`{ "state": "running", "n": 1`) -> both a new run and `-Id ... -Status implemented` refuse with a message matching `recovery record .* is unusable`; the file is left untouched | 1 |
| harness-lock2.ps1 | (in-flight, near end of file) | LOCK2 (findings refused while `.consult.lock` is held by another process) | while a background consultation run holds the task's `.consult.lock` (mid-run, `state=running` pending record present): `codex-findings -Id F01-1 -Status implemented` is refused, output matching `held open by pid \d+` | 1 |
| harness-lock2.ps1 | (in-flight, near end of file) | LOCK2 (`-List` still works under the same lock) | in the same in-flight window, `codex-findings -List` succeeds (it only reads, taking no lock) and its output contains `pending: state=running`, describing the in-progress run | 0 |
| harness-panel.ps1 | RUN | RUN (`-Rate` goes through the store commit) | `-Rate` acquires the write-lock commit path (like a status change): after it returns, exactly one rating is recorded, the findings count is unchanged (3), and the task's `.consult.write.lock` is free | 0 |
| harness-panel.ps1 | NOLOSS | NOLOSS (panel consultations leave no orphans) | after a 4-member panel run, the ledger has all 4 entries sorted by n; `-List -All` reports `orphans: 0 finding(s), 0 reviewer check(s)` | 0 |
| harness-panel.ps1 | INFLIGHT | INFLIGHT (findings status refused during a panel run, naming the panel) | while a panel run holds the task lock, both a solo consultation and `codex-findings -Status` are refused on the same lock, the refusal naming `held open by pid <N> ... (review panel <shortid>)`; `-List` in the same window shows one `pending:` line per panel member's recovery record | 1 |
| harness-panel.ps1 | TIMEOUT | TIMEOUT (`-Rate` refused while a survivor process lives) | a killed panel member's process survives (still running) -> both the next run and `codex-findings -Rate` are refused, `-Rate`'s message starting `a previous consultation's codex process (pid <N>) is still running`, and no `findings.json` is created by the refused `-Rate` | 1 |
| harness-panel.ps1 | BLOCKED | BLOCKED (findings status refused when the write lock itself is contended) | with `.consult.write.lock` held by another process, `codex-findings -Status` fails with a message matching `^codex-findings: the write lock '.*' of task '.*' was not acquired within .* s: .*; nothing was changed\.$`; the finding's status and the ledger are left unchanged | 1 |
| harness-panel.ps1 | ORPHAN | ORPHAN (a commit killed between findings.json and sessions.json) | a bridge process killed inside its own store commit (findings.json written, sessions.json not yet) leaves that finding as an orphan: no ledger entry, `-List -All` reports `orphans: 1 finding(s)`, the interrupted record's `state` stays `committing`, and the write lock is released with the dying process | n/a |

## Open questions

- **Status-transition matrix not directly harness-asserted beyond F04-3/panel's `-Status
  implemented`**: the PS1 script accepts any of `proposed \| implemented \| verified \|
  rejected \| wontfix \| superseded` as a target for any current status (validated in
  `codex-findings.ps1`'s own parameter checks: `-Status verified` requires `-Evidence`,
  `-Status rejected` requires `-Note`, and a reopen to `proposed` from any other status
  requires `-Note`), but no harness file exercises every transition/require-arg combination
  end to end — only `implemented` (via F04-3, harness-lock2, harness-pending, harness-panel)
  and the ratings machinery are exercised live. Whether C3's acceptance bar should include a
  new harness (or `c3 findings` unit tests) covering `verified`/`rejected`/reopen validation
  directly, since the reference plugin's own test suite does not, is a design decision for
  whoever finishes `c3 findings`.
- **`-All` vs default `-List` scope, and the "other" status bucket**: the PS1's `-List`
  summary line groups by every status in `FindingStatuses` plus an `other` bucket for any
  unrecognized status string; no harness row above exercises an unrecognized status reaching
  that bucket (it would require a hand-edited findings.json). Flagging as untested surface
  rather than a checklist gap to close silently.
- **Console-table column alignment for `-Stats`/`codex-scoreboard`**: unlike `codex-providers.ps1`
  (M1, which the harness pins to an exact regex including column alignment), the findings/
  scoreboard harnesses assert column *headers* and specific *row* regexes but do not assert
  full-table alignment invariants across every row the way PREFLIGHT's console table check
  does. C3's implementation only needs to match the asserted rows/headers, not a stricter
  alignment contract the reference plugin itself doesn't enforce by test.
- `c3 findings` and `c3 scoreboard` do not exist yet (being implemented in parallel by another
  worker), so none of the above has been run end-to-end against a live C3 binary; this
  checklist is unverified against `c3 findings`/`c3 scoreboard` and is derived purely from
  reading the plugin's scripts and harnesses.
