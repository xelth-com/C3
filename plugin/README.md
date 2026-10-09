# c3 (Claude Code plugin)

A Claude Code plugin (`c3`, version 0.2.0) — the packaging of the `c3` binary as a thin
layer for Claude Code: a `SessionStart` hook and three skills (`consult`, `coordinate`,
`setup-providers`) with brief and role templates, mirroring the
[`codex-consult`](https://github.com/xelth-com/claude-codex-consult) plugin's own layout.
This README is written for the AI coding agent that installs, wires and uses the plugin;
humans can follow the same steps.

The plugin itself carries no reviewer logic: every command it documents shells out to the
`c3` binary. See the [C3 repository](https://github.com/xelth-com/C3) for what `c3` is,
`docs/DESIGN.md` for the design, and `docs/port/cli-surface.md` for the full flag mapping
from the PowerShell plugin to `c3`'s subcommands.

## For the agent installing this

- **What it is:** a dependency-free Rust binary (`c3`) that runs `codex exec` for a
  review (or a routed panel, or a Gemini/Muse/OpenRouter reviewer through the `agy`/`muse`/
  `http` engines), records the consultation as files (brief, verbatim reply, JSON ledger),
  and manages reviewer identity, availability, structured findings, usefulness ratings,
  packs, a code index and panel routing — plus this thin Claude Code layer (a `SessionStart`
  hook and the `consult`, `coordinate` and `setup-providers` skills).
- **Prerequisites.** Check each with the command; do not assume:
  - [ ] the `c3` binary on PATH, or `C3_EXE` set to its path: `c3 --version`
  - [ ] git: `git --version` → `git version …`
  - [ ] Codex CLI on PATH: `codex --version` → `codex-cli 0.148` or newer
  - [ ] a reviewer: `codex login status` → `Logged in using ChatGPT`, **or** a
        `[model_providers.<name>]` table whose `env_key` variable the USER has set.
        Never create, print or paste an API key.
- **Install the plugin directory:** at the Claude Code prompt,
  `/plugin marketplace add xelth-com/C3`, then `/plugin install c3@C3` (once the C3
  repository publishes a `.claude-plugin/marketplace.json` at its root pointing at this
  `plugin/` directory, the same way `claude-codex-consult`'s root manifest points at
  `plugins/codex-consult`). Until then, point Claude Code at a local checkout:
  `/plugin marketplace add <path to this repository>`, then `/plugin install c3@<name>`.
- **Install the binary:**
  - from source: `cargo install --path crates/c3-cli` (from a checkout of the C3
    repository) puts `c3` on `PATH` via cargo's own bin directory; or
  - a release download, once published, placed anywhere on `PATH`, or pointed at with
    `C3_EXE=<path>`.
- **Verify:** `c3 providers` → at least one row `available`; then a `--dry-run`
  consultation → first line `DRY RUN - nothing was executed and no file was written.` and
  a line `preflight   : available (…)`. Exact commands: the `setup-providers` skill,
  sections 1 and 5.
- **First consultation:** invoke the `consult` skill with a task id and a question, or
  run the command under that skill's "Run one command" section directly.
- **More reviewers** (z.ai GLM, Xiaomi MiMo, any Responses-API provider): follow the
  `setup-providers` skill.

## How `c3 hook` and the skills relate to the PowerShell plugin

Both plugins can be installed side by side in the same Claude Code install, and even
point at the **same repository**: C3's `.collab` files (`sessions.json`, `findings.json`,
the `handoffs/` directory) are byte-compatible with the PowerShell plugin's — either one
can append to a task's ledger and the other reads it as its own history. They do not need
to be wired to the same provider or roster at once, but if both are enabled, only one
`SessionStart` hook line matters at a time in the agent's context (whichever plugin's hook
Claude Code runs); the underlying files stay shared regardless.

The `hooks/hooks.json` in this directory registers a `SessionStart` hook that runs
`hooks/c3-hook.ps1` (Windows PowerShell 5.1/7) or `hooks/c3-hook.sh` (macOS/Linux),
picking whichever interpreter is available the same way the PowerShell plugin's own hook
line does. Each wrapper locates the `c3` binary (the `C3_EXE` env override first,
then a binary bundled at `<plugin root>/bin/`, then PATH), runs `c3 hook`, and prints its
one-line output. If `c3` cannot be found, the wrapper prints one line saying so and still
exits 0 — a missing binary never blocks a Claude Code session from starting.

Platforms: the `c3` binary builds and passes its test suites on Windows 11 and on Ubuntu
x86_64 (the Linux port, `docs/port/linux-port.md`); the plugin's own files are the same on
both, and the `.collab` files a Linux `c3` writes are byte-compatible with the Windows ones.
macOS is untested.

The `skills/consult/SKILL.md`, `skills/coordinate/SKILL.md` and
`skills/setup-providers/SKILL.md` files are the C3 versions of the PowerShell plugin's
`consult-codex`, `coordinate` and `setup-providers` skills: same rules, every command
rewritten to `c3 <subcommand> --kebab-case-flag` form. Today's `c3` binary implements the
full working surface — the routed panel, detach/status/wait/kick, the `agy`/`muse`/`http`
engines, ratings, the scoreboard, packs, the index and the router — so the skills document
those directly; `c3 <subcommand> --help` is the authoritative current flag list, and
`skills/consult/reference.md` holds the detailed flag/engine/panel reference the short
skill points at. The one part the PowerShell plugin has that C3 deliberately does not port
is multi-host coordination (Codex CLI, Z Code, Kimi Code, a plain shell): C3's coordinator
host is Claude Code only.

## Telemetry

Installing `c3` means accepting its telemetry terms — one anonymous event per
consultation (outcome classes only, never task names, prompts, briefs, paths or keys), off
with `CODEX_CONSULT_TELEMETRY=off` or `--telemetry off` per run, never blocking a run. See
the C3 repository's root [README.md](../README.md), section "Telemetry", for exactly what is
sent, what never is, and how to inspect (`c3 telemetry status`), complain about
(`c3 complain "<text>"`) or delete (`c3 forget-me`) your data.

## The MCP server (an alternative surface, not shipped enabled)

`c3 mcp` runs a stdio MCP server that exposes C3's read-and-record tools (providers,
consult, findings, scoreboard, pack, explain, snapshot, index query) to any MCP client — the
surface for a host that does **not** load skills. This plugin does **not** declare or ship
that server: a Claude Code user already has the skills above, an always-on `c3 mcp` process
would give the model a second, unguided way to do what the skills already do (the same
double-surface problem as running two SessionStart hooks), and enabling the plugin should
start no background process. To use the MCP server anyway, add it opt-in with a repository-root
`.mcp.json` (`{"mcpServers":{"c3":{"command":"c3","args":["mcp"]}}}`) — see
`docs/port/mcp.md` in the C3 repository.

## License

MIT — see the C3 repository's [LICENSE](../LICENSE).
