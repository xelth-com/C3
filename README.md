# C3 — claude-codex-consult in Rust

**Status: the full surface runs.** The parity port (milestones 1–6) is in the binary —
`c3 providers`, `c3 consult` (with the `codex`, `agy`, `muse` and `http` engines),
`c3 findings` (statuses and ratings), `c3 scoreboard`, `c3 hook`, `c3 telemetry`/`complain`/
`forget-me`, the reviewer roster and the routed **panel** with detach/status/wait/prune/kick —
and so are the heavy subsystems after it: **packs and the `http` engine** (milestone 7,
run live against OpenRouter — see `docs/port/m7-status.md`), the **index** (milestone 8,
SurrealDB behind a default feature), the **router** (milestone 9) and the **stdio MCP server**
(milestone 10, `c3 mcp`). What is measured by which harness: milestone-1 parity by
`docs/port/m1-acceptance.md` (byte-for-byte against `codex-providers.ps1`) and the per-milestone
`m*-acceptance.md` / `m7-status.md` notes, plus `cargo test` (the c3 lib, `c3-core`, and the
`http_engine` integration suite incl. a seeded-secret grep of every written file) and
`cargo clippy -D warnings`. **Deliberately not ported: multi-host coordination.** The PowerShell
plugin coordinates from Claude Code, Codex CLI, Z Code, Kimi Code or a plain shell; C3's
coordinator host is **Claude Code only** (host-invariance across shells stays with the plugin).
The PowerShell plugin [`codex-consult`](https://github.com/xelth-com/claude-codex-consult)
remains installable (`/plugin marketplace add xelth-com/claude-codex-consult`, then
`/plugin install codex-consult@claude-codex-consult`) and its `.collab` files are byte-compatible
with C3's — either bridge reads the same task history. Its README is the specification this port
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
cargo build --no-default-features   # fast dev loop: drops SurrealDB (`c3 index` reports `none`)
```

The `index-surreal` feature (on by default) pulls in SurrealDB, which adds ~26 min to a
cold release build; `--no-default-features` compiles it out for a fast development loop and
CI builds both variants.

### Platforms

Windows 11 is where every harness ran and where the PowerShell plugin's evals are the oracle;
since the Linux port (`docs/port/linux-port.md`) the same tree builds, lints and tests on
Ubuntu x86_64 too, and `.github/workflows/ci.yml` runs fmt, clippy and the test suites on
`ubuntu-latest` and `windows-latest` (the SurrealDB build only on Ubuntu). What differs on
Linux: process liveness and the tree kill come from `/proc` and `kill` instead of the Win32
APIs and `taskkill /T`, the file locks are `flock` advisory locks instead of share modes, and
a detached run is a child in its own process group instead of a `cmd.exe` launch; the files
written are the same bytes. The tests that need Windows (cmd.exe launchers, Windows launcher
paths) are gated `#[cfg(windows)]` with a reason each. macOS is untested. For a sandbox whose
egress proxy carries the credential and re-terminates TLS, the `http` engine honours
`HTTPS_PROXY`, `C3_HTTP_AUTH_PROXY=<host,...>` (no `Authorization` header, no key read) and
`C3_HTTP_CA_BUNDLE=<pem>` (extra trust anchors) — see `docs/port/linux-port.md`.

A cargo workspace of three crates: `crates/c3-core` (contracts and file formats),
`crates/c3` (runtime modules), `crates/c3-cli` (the binary). Each ported piece brings its
own dependencies when it needs them; SurrealDB arrives with milestone 8 behind a feature.
The plugin's harnesses can drive the binary through `tests/shim/` (see
`docs/port/harness-shim.md`); `docs/port/m1-acceptance.md` lists the assertions milestone 1
answers to.

## Install into Claude Code

`plugin/` is a Claude Code plugin directory: a `SessionStart` hook and three skills
(`consult`, `coordinate`, `setup-providers`) with brief and role templates and an `evals/`
suite — a thin layer over the `c3` binary built above, carrying no reviewer logic of its own.
Install the binary first (`cargo install --path crates/c3-cli`, or a release download, on
`PATH` or pointed at with `C3_EXE=<path>`), then add the plugin directory in Claude Code
(`/plugin marketplace add`, then `/plugin install`, against this repository or a local
checkout). The plugin does **not** ship the `c3 mcp` server enabled — a skills-driven Claude
Code session already has the surface, and an always-on server would be a second, unguided way
in; add it opt-in with a repo-root `.mcp.json` (see `docs/port/mcp.md`). See
[plugin/README.md](plugin/README.md) for the exact commands, prerequisites and how it relates
to the PowerShell [`codex-consult`](https://github.com/xelth-com/claude-codex-consult) plugin
(both can be installed side by side; their `.collab` files are byte-compatible). The
switch-over procedure from the PowerShell plugin to C3 is
[docs/port/phase2-switch.md](docs/port/phase2-switch.md).

## License

MIT — see [LICENSE](LICENSE).
