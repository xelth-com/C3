# Phase 2 switch-over — installing the C3 plugin alongside (or in place of) `codex-consult`

This is the **supervisor's** runbook for putting the `c3` Claude Code plugin (`plugin/`) into
service. It is written so the switch is deliberate and reversible. **Nothing here has been run
on this machine** — two live sessions currently depend on the installed `codex-consult` plugin.
Only the supervisor (or the user) runs the state-changing commands below; the read-only ones
(`claude plugin list`, `claude plugin details`, `claude plugin validate`) are safe to run first.

All commands are the Claude Code CLI (`claude plugin ...`); the `/plugin ...` slash forms inside
a session are equivalent. A plugin change applies on the **next** session, not the running one.

---

## (a) What is installed today

Read it, do not assume (names only):

```
claude plugin list
```

At the time this runbook was written that prints two enabled, user-scoped plugins:

- `codex-consult@claude-codex-consult` — the PowerShell reviewer bridge (the one C3 replaces).
- `rust-analyzer-lsp@claude-plugins-official` — unrelated; leave it alone.

The user's own skills `astra` and `workers` (`~/.claude/skills/`) are **not** plugins but they
name the plugin's skills: `astra` invokes `codex-consult:consult-codex` and
`codex-consult:setup-providers` directly (with PowerShell `-Flag` conventions and
`-CollabDir .eck/collab`); `workers` only points at `codex-consult:consult-codex` in prose. This
matters for step (c).

---

## (b) Add and enable the C3 plugin from the local checkout

### b0. Prerequisite — the binary

The plugin shells out to `c3`; install it first (it is already on PATH here at
`~/.cargo/bin/c3`, built from `4e8d1a8`):

```
cargo install --path crates/c3-cli        # from a checkout of this repo; or a release download
c3 --version                              # expect: c3 0.1.0
```

Or point the plugin at a binary elsewhere with `C3_EXE=<path>` (the hook and skills honour it).

### b1. Prerequisite — the marketplace manifest (present)

`claude plugin install` installs from a **marketplace**, and `claude plugin marketplace add
<path>` expects a `.claude-plugin/marketplace.json` at that path. The repository has one at its
root since 2026-09-29: marketplace name `c3`, one plugin `c3` with `source: ./plugin`, so the
install id is `c3@c3`. Both manifests pass `claude plugin validate` (read-only, installs nothing):

```
claude plugin validate .            # the marketplace manifest
claude plugin validate plugin       # the plugin manifest
```

Once the repository is pushed with this manifest, `claude plugin marketplace add xelth-com/C3`
works against GitHub the same way.

### b2. Add the marketplace and install

```
claude plugin marketplace add C:\Users\Dmytro\c3          # the repo root (has .claude-plugin/marketplace.json)
claude plugin install c3@c3                               # <plugin name>@<marketplace name>
```

`install` enables it by default (user scope). Confirm:

```
claude plugin list                                        # c3@c3 → enabled
claude plugin details c3                                  # the hook + 3 skills + templates + evals inventory
```

---

## (c) What to do with the `codex-consult` plugin

Two options. **Recommendation: keep both enabled for now** and disable `codex-consult` only
later, deliberately, after C3 has soaked and the `astra` alias is repointed.

### Option 1 — keep both enabled (recommended, the parallel/soak period)

Consequences:

- **Two SessionStart hook lines** in every new session's context: `codex-consult: ...` and
  `c3: ...`. Both are single availability lines; the noise is one extra line, not a conflict.
- **Two skills answer the same request.** `codex-consult:consult-codex` and `c3:consult` both
  match "consult a reviewer" (likewise the two `setup-providers`); the model must choose. In
  practice this is contained: the user's `astra` skill explicitly invokes
  `codex-consult:consult-codex`, so Astra consultations stay on the PowerShell bridge, while C3
  is exercised through its own `c3:consult` / `c3:coordinate`.
- **`astra` and `workers` keep working** — they name `codex-consult:*`, which is still present.
- **No data conflict.** Both bridges write byte-compatible `.collab` files (DESIGN §3 invariant
  1); either can append to a task's ledger and the other reads it as its own history. They need
  not share a provider or roster.

### Option 2 — disable `codex-consult`

```
claude plugin disable codex-consult@claude-codex-consult
```

Consequences:

- **Clean surface** — one hook line, one answer per request (only `c3:*`).
- **`astra` breaks**: it invokes `codex-consult:consult-codex` / `setup-providers`, which no
  longer resolve; the `workers` skill's prose pointer dangles. Before disabling, repoint `astra`
  at `c3:consult` / `c3:setup-providers` and convert its `-Model` / `-CollabDir .eck/collab`
  conventions to `--model` / `--collab-dir .eck/collab` (editing `~/.claude/skills/astra/` is the
  user's action, out of this repo's scope).
- **PowerShell-only capabilities go away for new sessions**: multi-host coordination (Codex CLI,
  Z Code, Kimi Code, a plain shell) is not in C3 at all. C3 does carry the `agy`/`muse`/`http`
  engines, the routed panel, ratings and the scoreboard, so single-host reviewing is covered.
- **Running sessions are unaffected** until they restart; `disable` changes the next session.

**Why keep both first:** the C3 plugin is `0.1.0` packaging, the user's own skills are wired to
`codex-consult:*`, and two live sessions depend on the PowerShell bridge. Run them in parallel,
exercise C3 on real consultations, repoint `astra`, then disable `codex-consult`.

---

## (d) Verify the switch in a NEW session

Start a fresh Claude Code session in a git repository, then:

1. **The hook line.** The SessionStart context shows one line beginning `c3:` — e.g. the
   availability summary from `c3 hook`, or `c3: not installed - place the c3 binary on PATH ...`
   if the binary is missing (the hook always exits 0 and never blocks the session).
2. **A dry run writes nothing** (invoke the `consult` skill, or run directly):

   ```
   c3 consult --task switch-check --prompt "Reply with one sentence." --dry-run
   ```

   Expect exit 0, first line `DRY RUN - nothing was executed and no file was written.`, a
   `preflight   :` line and a `reviewer    :` line. No `.collab/switch-check/` directory is created.
3. **Providers.**

   ```
   c3 providers
   ```

   Expect exit 0, a `codex config:` line, an `endpoint health:` line, one row per provider with
   its verdict, and (with a roster) the `roster: ... would select ...` and `availability:` lines.
   `c3 providers --short` is the one-line form the hook uses.

Do **not** run a real consultation to verify — a dry run and `c3 providers` prove the wiring
without spending quota or making a provider call.

---

## (e) Rollback, command by command

If C3 must be backed out (any step is safe; each applies next session):

```
claude plugin disable c3@c3                    # stop using it, keep it installed
# or, fully:
claude plugin uninstall c3@c3                  # remove the plugin
claude plugin marketplace remove c3            # remove the local marketplace entry
claude plugin enable codex-consult@claude-codex-consult   # only if it was disabled in step (c)
claude plugin list                             # confirm the end state
```

The marketplace manifest at the repository root stays: it is part of the repository. The `c3` binary can stay on PATH; it does nothing unless invoked. No `.collab`
files need cleanup — they are shared and valid for the PowerShell bridge too.

---

## (f) What changes for the other live session that uses the plugin

- **Nothing changes mid-session.** A plugin add/enable/disable is read at session start; a
  session already running keeps the skills and hook it loaded until it restarts.
- **On its next restart**, if both plugins are enabled it will see both hook lines and both
  `consult`/`setup-providers` skills (Option 1 above) — its consultations still resolve, and any
  `astra`/`workers` invocation still finds `codex-consult:*`.
- **If `codex-consult` was disabled** (Option 2), that session on restart loses the
  `codex-consult:*` skills; any workflow that invokes them by name (notably `astra`) must be
  repointed at `c3:*` first, or it will fail to find the skill.
- **The shared `.collab` files stay valid for both** — a task the other session started under the
  PowerShell bridge is readable and appendable by C3 and vice-versa, so no in-flight task is
  stranded by the switch.
