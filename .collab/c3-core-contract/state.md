# c3-core-contract - state

Task: core-contract review of `crates/c3-core` (base `e1acae1`) before engines (M2c) and the ledger runtime (M3) build
on it. Coordinator and judge: Claude Code (Fable 5.1).

## Consultations

| n | reviewer | outcome | findings | rated |
|---|---|---|---|---|
| 1 | openai :: gpt-6-astra | ADVISE, 3 blocker / 12 major / 2 minor | F02-1..17 | yes |
| 2 | ZAI :: glm-5.3 | ADVISE, 1 blocker / 3 major / 4 minor / 2 note | F03-1..10 | yes |
| 3 | mimo :: mimo-v2.6-pro | ADVISE, 2 blocker / 6 major / 4 minor | F04-1..12 | partly |
| 4, 5 | byteplus deepseek / dola | failed: 429 (plan limit) | - | not rated |
| 6 | kimi :: k3 | ADVISE, 1 blocker / 4 major / 2 minor / 1 note | F07-1..8 | yes |
| 7 | alibaba :: qwen3.8-max | ADVISE, 4 blocker / 5 major / 3 minor / 2 note | F08-1..14 | yes |
| 8 | meta :: muse-spark-1.3-contributor | ADVISE, 1 blocker / 5 major | F09-1..6 | partly |

Gemini entries skipped (Antigravity quota until 2026-09-28 21:30). All six verdicts ADVISE; nobody said "freeze as is".

## Themes (de-duplicated by a fresh-context worker, every code citation re-opened and confirmed)

| theme | claim | max sev | raised by | decision |
|---|---|---|---|---|
| T1 lock guard | `commit` takes no write lock; `take_task_lock` is an ordinary open; order unenforceable | blocker | 6/6 | `take_write_lock -> WriteLock`, `commit(&WriteLock)`, real exclusive opens, measured wait (A) |
| T2 schema tolerance | unknown members dropped on rewrite, closed enums, required fields reject legacy/fresh plugin stores | blocker | 6/6 | `serde(default)` + flattened `extra` maps, tolerant status tokens, legacy fixtures (D) |
| T3 pending records | single-run filename hard-coded; member records never written/removed; commit deletes recovery unconditionally | blocker | 5/6 | `PendingRef`, `RecoveryDisposition`, full pending record incl. recovery pointers (C) |
| T4 fork | codex `Mode::Fork(t)` drops the parent id | blocker (GLM) | 5/6 | fix + exact argv test (E) |
| findings delta | `commit` overwrites the whole findings snapshot | blocker (Astra, MiMo) | 2/6 + folded | `FindingsDelta` applied after re-read under the lock; findings-only transactions (B) |
| T5 engine seam | no PromptDelivery, TurnKind, precheck; `Reply`/`continue_turn` carry no failure or replay context | major | 6/6 | the Q2 bundle: PromptDelivery, TurnKind, LaunchPlan, precheck, TurnRequest, AttemptOutcome (E) |
| T6 header slots | one `extra` slot; the plugin interleaves optional records at ~6 positions | major | 5/6 | typed ordered slots mirroring `$headerLines`, read from the plugin source (G) |
| T7 path containment | task and artifact paths joined unchecked | major | 4/6 | `TaskSlug`, contained relative paths (G) |
| T8 reply validation | `StructuredReply` accepts forbidden enum values, missing line; deny_unknown_fields blocks additive v2 | major | 4/6 | raw DTO vs validated reply, schema_version first (F) |
| T9 idempotence | pending removal after the sessions rename; `add_entry` never dedupes | major | 3/6 | dedupe by consult_id/n, `CommitReceipt` (G) |
| T10 bootstrap | first commit writes empty `cwd`, `tool` from the engine harness | major | 3/6 | inputs (G) |
| T11 transitions | free `transition()` touches no history; `status` public; `supersedes` typed as Value | minor | 4/6 | atomic `set_status` (G) |
| T12 http lineage | engine table has three rows, `EngineKind` four | major | 2/6 | single-sourced table with http (E) |
| T13 lineage on resume/fork | bare strings, crossing lineages uncheckable | note | ~5/6 | lineage key on Resume/Fork (E) |
| T14 agy/muse fork | silently plan a fresh launch | major | 2/6 | `UnsupportedMode` (E) |
| S2 TOML escaping | `-c` values unescaped | minor | Astra | shared basic-string encoder (E) |
| S4 next numbers | second half of `Get-NextNumbers` and `write_raw_reply` missing | major | Qwen | added (G) |
| S1 ps_json Unicode/floats | parity fails for non-ASCII keys and exotic doubles | minor | Astra | narrow the documented guarantee; no fixture shows the plugin emits them |

## Decisions on the eight questions

Q1 canonicalise to PS 5.1 on write, read any (all six), with the reader made genuinely tolerant (T2). Q2 keep
`plan`/`run`/`continue_turn`, add the bundle (T5). Q3 rules stay in the runtime; core adds validating constructors and a
lineage key. Q4 the handoff is derived once at write time; the ledger order string is not widened; typed slots instead.
Q5 raw pass-through is the interim reader contract; each non-null literal is pinned from the plugin source before C3
first writes it (`peak` shape to be read, not assumed). Q6 confirmed: pack path and hash in `reviewer.provider_config`
plus the `.pack.json` sidecar, never `artifacts[]`, never a ledger column. Q7 the split was unsafe, not deferred: the
guard is in the trait signature (T1). Q8 schema-extension tolerance, worktree identity, cancellation ownership, Windows
path containment, R12 kept out of the ledger until frozen.

Contradictions settled: fork severity treated as must-fix; schema tolerance escalated to blocker (Kimi, Qwen); the
`http` engine will be its own `Engine` implementation in M7 with `EngineKind::Http` in the shared table; `peak` shape
settled by reading the plugin literal during the fix.

## Requested checks

RC1 (Astra) store fault injection with two writers - covered by the lock and delta tests of the fix; the cross-process
case runs in M3. RC2/RC (all) `cargo test -p c3-core` - part of the fix verification. RC3 (Astra) differential
round-trips of legacy/extended stores - part of decision D. Fork parity, `Get-NextNumbers`, `New-LedgerFile|cwd` greps
(GLM, Kimi) - the fix worker reads those literals. Fixtures for unknown keys, `deferred` status, fresh findings without
`ratings` (Kimi, Qwen) - part of decision D.

## Status

2026-09-26 22:20: fix set A-G dispatched (task M2b).
2026-09-26 22:55: fix set landed - 77 tests (39 lib + 8 contracts + 22 formats + 7 store in c3-core, 1 in c3), clippy
and fmt clean, `c3 providers` unchanged. 63 findings moved to `implemented`; F02-16 `wontfix` (parity guarantee
narrowed instead); F04-8 (input digest) and F08-14 (full gating in Request) partial, stay `proposed`; F07-7 (non-UTF-8
argv paths) deferred, stays `proposed`. Plugin literals settled the open shapes: `peak` is `boolean|null` (Astra was
right), `provider_failure` has 7 fields (`kind`, `hint` omittable), the write lock passes `FileShare.Read` (not None),
fresh `findings.json` is `{task_id, findings}` with `ratings` added lazily. `verified` waits for the cross-process lock
and fault-injection runs of M3 (Astra RC1) and the harness runs of M2c.
