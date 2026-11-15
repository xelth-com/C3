# release-0.2.4 - state

- 2026-11-15: diff-review panel of the 0.2.4 wave (c3 maintain, grok prompt-only transport and
  fresh-conversation re-ask, z.ai / Muse quota sources, build commit in `--version`).
  - Brief: `.collab/release-0.2.4/handoffs/01-claude-diff-review.md`; the change as a patch:
    `.collab/release-0.2.4/handoffs/01-claude-diff-review.patch` (base 035d386 + working tree).
  - Panel: detach id `76fe4406` - ZAI-claude :: glm-5.3 (handoff 02), meta :: muse-wonder-1.0 (03),
    grok :: grok-4.7 (04); seated through a scratch roster of exactly these three, because the
    standing partner (openai :: gpt-6-astra) sits at 97 % of its weekly window until Nov 18 and the
    native kimi engine reviews a HEAD worktree that does not hold the uncommitted change.
  - Come back: `c3 consult --task release-0.2.4 --status --id 76fe4406`.
  - Run with the debug build of the wave itself (`target\debug\c3.exe`), so the grok member already
    takes the schema in the prompt.
- 2026-11-15, result: panel wall 15 min, all three usable. GLM-5.3 ACCEPT (4 minor, 3 note); Muse Wonder HOLD
  (1 blocker, 10 major, 1 minor); Grok HOLD (5 major, 5 minor, 1 note) - Grok made 84 tool calls on its first turn,
  no re-ask (its first run on the prompt-only schema transport). 30 findings, none rejected: 24 `verified` by a named
  test in the green full run (c3-core 281, c3 lib 634, c3-cli 188 in 19 binaries), 6 `implemented` without a
  dedicated test (F03-9, F02-1, F04-7, F02-4, F04-8, F03-12). All three consultations rated useful.
  - One defect came from a fix itself: the direct `CreateProcessW` spawn asked for by F04-8 inherited the hook's
    output pipes, so the hook "returned" only when the pass ended (30 s, seen live). Fixed (the hook's own standard
    handles are made non-inheritable first); regression test `the_hook_returns_without_waiting_for_the_spawned_pass`
    fails without the fix and passes with it; live: hook 2.1 s with a due pass, the pass ran 20 s in the background,
    the next hook printed the report once.
- 2026-11-15, second wave on top of d7ce658 - reviewer-protection routing (operator: "астра постоянно на передке
  часто убивает её в ноль"; Muse Wonder only on his explicit request). Brief
  `handoffs/05-claude-routing-review.md`, patch `05-claude-routing-review.patch`. Diff-review panel detach id
  `01480f5d`: meta :: muse-spark-1.3-contributor and grok :: grok-4.7 (the cheap pair, by the operator's rule; no
  Wonder, no Astra), run from a copy of the wave's debug build. Come back:
  `c3 consult --task release-0.2.4 --status --id 01480f5d`.
  Result: both HOLD (Grok 2 major / 2 minor, 14 min; Muse contributor 2 major / 1 minor, 4 min), 7 findings
  F06-1..4, F07-1..3, all held and fixed (`implemented`, each with a named test except F07-3); both rated useful.
  Full run after the fixes: c3-core 282, c3 lib 644, c3-cli 196 in 20 binaries, 0 failed.
- Acceptance by the standing partner: brief READY at `handoffs/08-claude-acceptance.md`, NOT run. On 2026-11-15
  the ChatGPT account was reset (weekly 25 % until Nov 22) but another session had its 5-hour window at 76 % until
  21:08, so a full acceptance would have hit the budget stop; and spending the standing reviewer is the operator's
  call the day he complained about it being drained. To run it:
  `c3 consult --task release-0.2.4 --purpose acceptance --mode new --brief .collab/release-0.2.4/handoffs/08-claude-acceptance.md --reply-name acceptance --detach`
  (no `--provider`: acceptance goes to the standing partner). Then `git tag v0.2.4` at the accepted commit.
- 2026-11-15 evening, on the operator's word ("запускай астру на приемку и пуш все что надо должна остаться одна
  мейн ветка"): the CI wave before the acceptance. GitHub CI had been red at its Format step since 2026-11-08
  (run 42525901306), so clippy and both Linux jobs had not run on any commit after 5018ac4. Done: `cargo fmt --all`
  (24 files), 9 clippy fixes without behaviour change (the roster engine-name regex built once before the loop,
  `budget/mod.rs`, `cli/{budget,router}.rs`, `engines/{grok,kimi}.rs`, `maintain/version.rs`, two tests), the Linux
  cfg gates of everything added since 5018ac4 read (no gap found; not compiled locally). Full run on the final tree:
  c3-core 282, c3 lib 644, c3-cli 196 in 20 binaries, 0 failed; fmt check and clippy `-D warnings` clean.
  CHANGELOG gained the entries the three 2026-11-08 commits never got (budget-aware panel routing, the native kimi
  engine, read-insist re-asks, the roster loader skipping an unknown engine); README gained kimi, `c3 maintain` and
  the `ext.reserve_percent` / `ext.on_request` settings. Brief 08 now names the base as `5018ac4..HEAD` and says
  outright that the 2026-11-08 commits were never reviewed by any model (new Q0).
  Branches: all 33 local branches and the two remote ones (`cloud/linux-port`, `claude/sharp-cannon-ocamev`) were
  fully merged into main and are deleted; the leftover agent worktree is removed. Only `main` remains.
- Acceptance authority: these three are stand-ins. Their verdicts are recorded; the tag `v0.2.4`
  waits for the standing partner's acceptance after its window resets (rule of the consult skill).
