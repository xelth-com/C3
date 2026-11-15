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
- Acceptance authority: these three are stand-ins. Their verdicts are recorded; the tag `v0.2.4`
  waits for the standing partner's acceptance after its window resets (rule of the consult skill).
