# Porting eckSnapshot into C3 (design v1, 2026-10-30)

Operator decisions (2026-10-30): C3 becomes self-sufficient - every eckSnapshot feature this
workspace uses moves into C3, and eck stops being a runtime dependency. C3 COMMITS, but only
through `c3 finish`, an explicit coordinator call (the old rule "C3 never commits" becomes "C3
commits only via `c3 finish`"). The features below under "Dropped" are not ported.

Source inventory: eckSnapshot 6.9.0 (`../eckSnapshot`), read 2026-10-30. In this workspace eck's
own telemetry counted 39 `eck_update_auto` (the finish-task delta) and 1 full snapshot; the
three MCP tools `eck_finish_task`, `eck_fail_task`, `eck_manifest_edit` are the only hard
runtime dependency (`.mcp.json`, `.codex/config.toml`, `opencode.json` point at
`eckSnapshot/scripts/mcp-eck-core.js`).

## Part A - finish / fail / manifest (replaces the eck MCP server)

`c3 finish --message <msg> (--status <text> | --status-file <path>) [--keep <n>=10] [--no-snapshot]`

1. Write `.eck/lastsnapshot/AnswerToSA.md` = `# Agent Report\n\n<status>\n`.
2. Prepend a journal entry to `.eck/JOURNAL.md` (create it with `# JOURNAL` when missing):
   `## <YYYY-MM-DD HH:MM> - <first line of the message>\n\n<status>\n` right after the title, so
   the newest entry is first (eck's order).
3. Stage: `git add -A` (it honours `.gitignore`), then UNSTAGE the never-commit set:
   `.collab/**/events.jsonl`, `.collab/**/*.events.jsonl`, `.collab/**/.admission.lock`,
   `.collab/**/.consult.lock`, `.collab/**/.consult.write.lock`, `.collab/**/.consult.pending*.json`,
   `.collab/**/work/**/.log`, `.collab/**/work/**/.stderr`. (`Cargo.lock` and every other lock
   file outside `.collab` are NOT in the set; `.consult.detached-*.log` stays committed - the
   repository's convention.)
4. Secret gate: the ADDED lines of every staged text file (`git diff --cached -U0`, every `+`
   line inside a hunk) pass the pack redactor (`crates/c3/src/pack/redact.rs`) - an unchanged
   committed test fixture no longer re-blocks every later commit of its file; a secret-shaped
   value refuses the commit (exit 1, the file and the KIND listed, never the value; nothing is
   unstaged so the coordinator can look). Fail closed: a staged list or a diff git cannot show
   refuses the commit as `unreadable`. `--allow-secret <path>` (repeatable) lets one reviewed
   file through.
5. Print the staged list (`A/M/D path`), then `git commit -m <msg>` (the message as given - the
   coordinator adds its trailers). Nothing staged: refuse with "nothing to commit" unless
   `--allow-empty`.
6. Unless `--no-snapshot`: a delta snapshot since the anchor (Part B), the report embedded at its
   top as `MESSAGE FROM THE EXECUTION AGENT`, written to `.eck/snapshots/` and copied into
   `.eck/lastsnapshot/` (the previous snapshot there removed, `AnswerToSA.md` kept); snapshots
   in `.eck/snapshots/` beyond the newest `--keep` are deleted.
7. Output: the commit hash, the staged count, the snapshot path (or why none), as text and with
   `--json`.

`c3 fail (--status <text> | --status-file <path>) [--keep <n>]`: `AnswerToSA.md` as
`# Agent Report (BLOCKED/FAILED)\n\n<status>\n`, the journal entry, a `--fail` delta (Part B)
with the report embedded; NO staging, NO commit. With no change since the anchor it still writes
the report and says "no snapshot: nothing changed" (eck printed "undefined" there).

`c3 manifest --file <NAME.md> --action replace_text|append_to_section|replace_file --content <t>
[--section <header line>] [--target <text>]`: the file must be a plain name ending `.md` directly
under `.eck/` (no separator, no `..` - eck allowed traversal); missing file starts as
`# <NAME>\n\n`; `replace_text` replaces the first occurrence and refuses when `--target` is
absent; `append_to_section` inserts at the END of the section (before the next header of the
same or higher level - eck inserted right after the header, which reversed entry order),
appending `\n<header>\n<content>\n` at the end of the file when the section is missing;
`replace_file` writes the whole content. No git, no snapshot.

MCP: `c3 mcp` gains `c3_finish_task {status, message}`, `c3_fail_task {status}`,
`c3_manifest_edit {file, action, content, section_header?, target_text?}` - eck's argument names,
so a rule that names the eck tools changes by name only. `c3_finish_task` is a supervisor-only
tool, as eck's was: its description says so.

## Part B - snapshot additions

- Output: `c3 snapshot` keeps its own anchor/sequence under `<collab>/.c3/`; `--eck` (the
  default for `finish`/`fail`) writes the file into `.eck/snapshots/` and copies it into
  `.eck/lastsnapshot/` (the architect's hand-off folder), prunes beyond `--keep`.
- `--base <ref>`: the delta from a commit hash, or from the anchor recorded in the header of a
  previous snapshot file.
- `--fail`: the delta from the anchor to the WORKING TREE (uncommitted changes included).
- `--embed-report <file>`: the report text at the top of the snapshot.
- `.eckignore` (gitignore syntax, repository root) is honoured by discovery next to
  `.gitignore`.
- Profiles: `--profile a,b,-c` reads `.eck/profiles.json` (`{name: {include: [globs], exclude:
  [globs]}}`); a leading `-` subtracts a profile; a bare glob is an ad-hoc include.

## Part C - project setup

- `c3 init [--force-stubs]`: creates the `.eck` manifests that are missing (CONTEXT,
  ARCHITECTURE, ROADMAP, TECH_DEBT, JOURNAL, OPERATIONS, ENVIRONMENT, RUNTIME_STATE,
  DEPLOY_CHECKLIST) with a `[STUB: ...]` marker; adds `.eck/` to `.gitignore` when absent;
  writes `CLAUDE.md`, `.claude/rules/01-c3-protocol.md`, `.claude/rules/02-delegation.md`,
  `.claude/agents/{sonnet,opus,haiku}-worker.md` ONLY when missing (never overwrites; lists what
  it skipped); registers `c3 mcp` in `.mcp.json` (adding, never removing other servers).
- `c3 doctor`: `[STUB` markers left in `.eck/*.md`, whether `c3 mcp` is registered, eck
  leftovers (an `eck-core` MCP entry, rules that name `eck_*` tools), the snapshot folder size.
- `c3 scout <path> [--depth <0-9>=0]`: a snapshot of ANOTHER directory into
  `.eck/scouts/scout_tree_<name>_<ts>.md` with a header on how to fetch more; `c3 fetch <glob>...`:
  full contents of matching files of THIS repository into `.eck/scouts/scout_data_<ts>.md`.
  Both pass the redactor.
- The plugin's skills get `c3-scout` / `c3-fetch` counterparts of eck's.

## Migration of this repository (last step)

`.mcp.json` registers `c3 mcp` and drops `eck-core`; `.claude/rules/01-eck-protocol.md` is
replaced by `01-c3-protocol.md` (same role text, `c3_finish_task` instead of `eck_finish_task`);
`CLAUDE.md` names C3; `.codex/config.toml` / `opencode.json` lose the eck entry. Then
`c3 doctor` must report no eck leftovers.

## Dropped (operator-approved)

NotebookLM exports (`booklm`, `notelm`, `notebook`); architect variants (`fable`, `--jas`,
`--jao`, `--jaz`, `--with-ja`); token-polynomial training and weight sync; ML model header peek;
the GLM-zai MCP worker (`c3 work` replaces it); OpenCode/AGENTS.md generation; `claudeSettings`;
agent-report telemetry; restore (dead code in eck); project-type detection (`detect`).
