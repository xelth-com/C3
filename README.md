# C3 — claude-codex-consult in Rust

**Status: early port.** Milestone 1 is done: `c3 providers` reproduces `codex-providers.ps1`
(table, `--short`, `--json`, exit codes) byte for byte on the same machine; every other
subcommand is a stub that says which milestone brings it. The working bridge today is the
PowerShell plugin [`codex-consult`](https://github.com/xelth-com/claude-codex-consult)
(install in Claude Code: `/plugin marketplace add xelth-com/claude-codex-consult`, then
`/plugin install codex-consult@claude-codex-consult`). C3 is that bridge rewritten in Rust
from a finished copy of it; the plugin stays, and its README is the specification this port
follows section by section. Page: <https://xelth.com/C3/>.

## What it is

A coordinator — your Claude Code, Codex CLI or any shell session — asks a **roster of
reviewers from different labs** to look at a one-page brief. Reviewers are read-only:
nothing is edited by them. Every consultation is recorded as files next to the code (the
brief, the reviewer's reply verbatim, a JSON ledger, **findings tracked by id** that you
verify and move yourself), and a **panel** runs several reviewers in parallel and
reconciles them. Reviewers are subscriptions the user already has: the ChatGPT plan through
the Codex CLI, z.ai GLM, Xiaomi MiMo or any Responses-API provider through a
`[model_providers.<name>]` table, Gemini through Google's Antigravity CLI `agy`, Meta Muse
through the Muse Code CLI. The bridge never creates, prints, stores or commits a key. C3 adds
one API path for people who will not juggle subscriptions - OpenRouter (or any
OpenAI-compatible endpoint) through a native `http` engine that reads its key from the
environment and sends the reviewer a prepared pack instead of tools - and, on top of the
same files, packs for chat models, a local SurrealDB index and telemetry-driven panel
routing. The design is in [docs/DESIGN.md](docs/DESIGN.md).

## Why a Rust rewrite

One static binary instead of PowerShell 5.1/7 on three OSes; a typed ledger and findings
store; the same behaviour under Claude Code, Codex CLI and a plain shell without a host
adapter (plugin ROADMAP R13, host invariance); and a place to make the bridge fast enough
to run a panel non-blocking (R12).

## Plan — port order (each step lands with tests and the plugin's evals as the oracle)

1. **Providers and preflight** — the Codex config walk (`[model_providers.*]`, `env_key`),
   `codex login status`, the roster file, the availability verdicts of `codex-providers.ps1`.
2. **One consultation** — brief → `codex exec` (and the `agy` / `muse` engines), the reply
   file with its header lines, the events stream, the `-DryRun` output byte for byte.
3. **Ledger and findings** — `sessions.json`, `findings.json`, ids, statuses, ratings,
   the atomic write order and the lock/recovery rules.
4. **Roster, panel, purposes, effort vocabularies, peak windows.**
5. **Telemetry and complaints** (plugin ROADMAP R17) — the terms below, the spool, `--complain`,
   `--forget-me`.
6. Claude Code packaging (skill, hook, evals) as a thin layer over the binary.

Steps 1-6 are the parity port. The heavy subsystems are explicit milestones after it
(see [docs/DESIGN.md](docs/DESIGN.md), section 10):

7. **Packs and the `http` engine** — snapshots and reviewer/explainer packs from one
   sanitizing builder; OpenRouter reviewers get a pack, never tools.
8. **Index** — embedded SurrealDB (per project, or one server per user across projects),
   rebuildable from files + repo, never required for a consultation.
9. **Router** — panel thickness (`shadow` / `council` / `consilium`) and a versioned,
   auditable draw from the scoreboard with priors distributed by the maintainer's server.
10. **MCP server** — read-and-record tools for Claude Code without the plugin.

## Telemetry (on by default, off with one line)

Installing C3 means accepting these terms. C3 reports **one anonymous event per
consultation** to the maintainer's intake, <https://xelth.com/T/>, so the bridge can be
improved when someone the maintainer never hears from uses it. Everything collected is shown
in the open, as counts, on <https://xelth.com/C3/>.

* **Sent:** `app_id` (`c3`), the C3 version, an instance id
  (`sha256(salt file + machine name)` — stable per installation, meaningless elsewhere),
  `event_type: consultation`, a severity, and `details`: engine, provider label, model,
  purpose, outcome class (`usable`, `failed:<class>`), wall seconds, token counts, findings
  counts, structured or not, a format retry or not, panel size, OS, runtime.
* **Never:** task names, prompts, briefs, paths, thread ids, finding texts, user names, keys.
  The server drops secret-named keys on arrival and reports how many; C3 treats a non-zero
  count as its own bug.
* **Off:** `CODEX_CONSULT_TELEMETRY=off`, or `--telemetry off` per run. The first run after an
  install prints this notice once; the dry run says whether it is on.
* **Never blocks a run:** events queue in a local spool (NDJSON under the config dir), are sent
  in the background with a 3 s timeout, retried on the next run and dropped after 7 days.
* **Complaints:** `c3 --complain "<text>"` prints the exact payload (your text plus the last
  ledger entry's summary), asks before sending, and prints a public reference like
  `T-7KQ4-M2XZ` to quote on the forum section <https://xelth.com/F/p/c3> or in a mail.
* **Delete my data:** `c3 --forget-me` (or
  `DELETE https://xelth.com/T/v2/instances/<instance_id>?public_ref=<ref>`) removes every
  event and complaint this installation ever sent.

Client rules and a 30-line Rust reference client:
[xelth.com/docs/t-hub](https://github.com/xelth-com/xelth.com/tree/main/docs/t-hub).

## Build

```sh
cargo build --release          # target/release/c3
./target/release/c3 --version
./target/release/c3 providers --short   # the reviewer availability line
cargo test && cargo clippy --all-targets -- -D warnings
```

A cargo workspace of three crates: `crates/c3-core` (contracts and file formats),
`crates/c3` (runtime modules), `crates/c3-cli` (the binary). Each ported piece brings its
own dependencies when it needs them; SurrealDB arrives with milestone 8 behind a feature.
The plugin's harnesses can drive the binary through `tests/shim/` (see
`docs/port/harness-shim.md`); `docs/port/m1-acceptance.md` lists the assertions milestone 1
answers to.

## License

MIT — see [LICENSE](LICENSE).
