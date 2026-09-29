---
name: setup-providers
description: Wire reviewers for the c3 bridge on a machine - verify the Codex CLI and its login, add a [model_providers.<name>] table for a third-party plan (z.ai GLM, Xiaomi MiMo or any Responses-API provider) with an env_key the user sets, supply per-run model catalogs, write the reviewer roster, and verify with `c3 providers` and a dry run. Use when a provider is missing or unavailable, on a new machine, or when the user asks to add a reviewer.
argument-hint: "[provider name, e.g. ZAI or mimo]"
allowed-tools: Bash(c3:*), Bash(codex:*), Read, Write, Edit, Glob, Grep, WebFetch
disable-model-invocation: false
---

# Set up reviewers for c3

This is the C3 port of the `codex-consult` plugin's `setup-providers` skill: the same
procedure, verified with the `c3` binary instead of `codex-providers.ps1` /
`codex-consult.ps1`. A reviewer goes through `codex exec` (the only engine `c3` actually
runs today — see "Not yet implemented" below); the bridge itself never makes an HTTP call.
`<codex home>` is `$CODEX_HOME` when set, else `~/.codex`.

**Not yet implemented in `c3`** — do not wire these expecting them to run a consultation:

- **Gemini through the `agy` engine** and **Meta Muse through the `muse` engine**
  (sections 3b/3f of the plugin's own skill). `c3 consult --engine agy` and
  `--engine muse` parse but do not run a reviewer (milestone 2d/4). `c3 providers` may
  still show a roster row for such an entry as `not checked` / `unknown`; that is the
  provider walk, not a working consultation path.
- **The reviewer roster's panel weighting and `-Panel`.** Write the roster file (section
  4 below) for `c3 providers`' own bookkeeping if you like, but a `--panel` run on `c3
  consult` is refused (milestone 4) — every consultation today is a single
  `--provider`/`--model` run.
- **BytePlus ModelArk, Kimi Code, Alibaba Cloud Model Studio** as documented providers —
  these are ordinary `[model_providers.<name>]` tables (same shape as z.ai/MiMo below) and
  there is no reason `c3`'s provider walk would treat them differently, but they have not
  been exercised against `c3` specifically; verify with `c3 providers --provider <name>`
  before relying on one.

## Invariants (never break these)

1. **Keys never go into files, output or chat.** You do not create, print, echo, log,
   commit or paste an API key, and you do not read one back from the environment. The
   user sets it; you only check that it is set, through `c3 providers`.
2. **One thread = one provider and model.** Never fork or resume a thread under another
   provider or model; `c3 consult` refuses it, and you do not work around it.
3. **Preflight is fail-closed.** A refusal (`provider X is not usable: …`,
   `availability could not be established`) is information, not an obstacle. Never
   pass `--skip-preflight` to get past a real refusal.
4. **The roster never replaces an available primary.** It orders the reviewers the user
   is willing to use; a fallback reviewer's reply is never presented as the primary's.
5. **Never invent** a base URL, a model name, a region or a peak schedule. Take them from
   the provider's official page or from the user.
6. **Ask before** installing software, editing `config.toml`, downloading a catalog, or
   running a first live consultation (it spends quota).

## 1. Verify the Codex CLI and the login

```
codex --version                 # expect: codex-cli 0.148 or newer
codex login status              # expect: Logged in using ChatGPT
```

Missing CLI: ask the user to install it (https://github.com/openai/codex). Not logged in:
ask the user to run `codex login` in their own terminal. The built-in `openai` provider
needs no table. Then check the config's top level without printing the file (it may hold
a token):

```
$cfg = Join-Path $(if ($env:CODEX_HOME) { $env:CODEX_HOME } else { "$HOME/.codex" }) 'config.toml'
Select-String -Path $cfg -Pattern '^\s*(model|model_provider|profile|model_catalog_json)\s*=', '^\s*\['
```

Want: a top-level `model = "…"` (without it, a run with no `--model` and no roster entry
has an unresolved identity and can only start new threads); no top-level `profile` (it
disables `fork`/`resume` for every run); no top-level `model_catalog_json` (it replaces
Codex's own catalog and degrades unrelated models). Ask the user before changing any of
them.

## 2. Add a provider table

Read the provider's official Codex page (WebFetch is fine) and take `base_url` and
`wire_api` from it:

- z.ai GLM Coding Plan: https://docs.z.ai/devpack/tool/codex (page shows
  `base_url = "https://api.z.ai/api/v1"`, `wire_api = "responses"`)
- Xiaomi MiMo Token Plan: https://mimo.mi.com/docs/en-US/tokenplan/integration/codex-configuration
  (page shows `https://token-plan-cn.xiaomimimo.com/v1`; the user's plan console names
  their region, e.g. `token-plan-ams`; pay-as-you-go `https://api.xiaomimimo.com/v1`)

Both pages put the key into the file as `experimental_bearer_token`. Replace that line with
`env_key`. Shape, appended to `<codex home>/config.toml` after the user agrees:

```toml
[model_providers.ZAI]
name = "Z.ai GLM Coding Plan"
base_url = "https://api.z.ai/api/v1"
env_key = "ZAI_API_KEY"
wire_api = "responses"
```

Conventions:
- The table name is the provider name everywhere (`--provider ZAI`, roster
  `"provider": "ZAI"`, `CODEX_CONSULT_PEAK_ZAI`); case-sensitive, short, ASCII. The env
  variable is `<NAME>_API_KEY` unless the user already uses another name.
- Plain single-line values only: no arrays, inline tables, multi-line strings, dotted keys
  or sub-tables inside the table (the bridge's scanner then marks the table unusable).
- Always write `wire_api = "responses"` explicitly. Changing `base_url` or `wire_api` later
  starts a new lineage (older threads can no longer be forked); `name`, comments and a
  rotated key never do.
- Only in the user-level `<codex home>/config.toml`: the bridge reads no other Codex
  config, so a table anywhere else is invisible to its identity and preflight.

Then tell the user to set the key themselves and restart Claude Code (a running session
does not see a variable set after it started):

- Windows: `setx ZAI_API_KEY "<key>"` in their own terminal
- macOS/Linux: `export ZAI_API_KEY="<key>"` in `~/.bashrc`, `~/.zshrc` or `~/.profile`

Never ask them to paste the key into the chat.

Models: use one the bridge's caps-v1 table declares for the host (z.ai: `glm-5.3`,
`glm-5.3-flash`, `glm-5.3-flashx`, `glm-5.2`, `glm-5.1`, `glm-5`, `glm-5-turbo`,
`glm-4.7`, `glm-4.6`, `glm-4.5`, `glm-4.5-air`; MiMo: `mimo-v2.6-pro`, `mimo-v2.6-flash`,
`mimo-v2.6-pro-ultraspeed`, `mimo-v2.5-pro`, `mimo-v2.5`). Any other model, or any host
not in caps-v1, needs `--native-effort <value>` on every run, which a roster entry cannot
supply.

## 3. Per-run model catalogs (MiMo-style)

Codex has no built-in catalog for MiMo models. With the user's consent, save the catalog
file the MiMo Codex page links to as `<codex home>/model-catalogs.json`, and pass it PER
RUN, never in `config.toml`: a global `model_catalog_json` replaces Codex's own catalog
and was observed to degrade the default `openai` model on an unrelated run ("Model
metadata not found, fallback"). Per run means the roster entry's
`"codex_config": ["model_catalog_json=~/.codex/model-catalogs.json"]`, or
`--codex-config model_catalog_json=~/.codex/model-catalogs.json` on one `c3 consult`
command (the bridge expands `~/`; Codex on Windows does not). The same rule applies to
any provider whose page suggests a global catalog (z.ai's suggests `~/.codex/models.json`;
the z.ai route works without one).

## 4. Write the roster

`<codex home>/codex-consult-roster.json`, first choice first:

```json
{
  "roster_version": 1,
  "reviewers": [
    { "provider": "openai", "model": "<the ChatGPT-plan model>", "panel": "weighty" },
    { "provider": "ZAI", "model": "glm-5.3" },
    {
      "provider": "mimo",
      "model": "mimo-v2.6-pro",
      "codex_config": ["model_catalog_json=~/.codex/model-catalogs.json"]
    }
  ]
}
```

- Allowed keys only: `roster_version` (must be `1`), `reviewers[]` with `provider`
  (required), `model`, `codex_config` (array of `key=value` strings), `auth`, `panel`,
  `engine` (`codex` is the only engine `c3` runs today — see "Not yet implemented"), the
  optional top-level `require` and `ext`. An unknown key, an unknown engine, a duplicate
  `(provider, model)`, or invalid JSON refuses EVERY run that reads the roster.
- `"panel": "weighty"` marks the expensive reviewer for a future panel run; it has no
  effect on a single-reviewer `c3 consult` run today.
- `codex_config` must not set `model`, `model_provider`, `model_reasoning_effort`,
  `profile` or `model_providers.*` (refused).
- Another file: the user sets `CODEX_CONSULT_ROSTER=<path>` (it must exist).
  `CODEX_CONSULT_ROSTER=none` disables the roster.

## 4b. Add an OpenRouter reviewer (the `http` engine)

The `http` engine sends one OpenAI-compatible `chat/completions` request built from a
reviewer pack — no CLI, no subscription, no tools. It is the path for someone who will not
juggle subscriptions and consults rarely: an API concentrator (OpenRouter by default) or
any endpoint that speaks the same wire shape.

**The key is never yours to touch.** The user sets the environment variable themselves
(e.g. `OPENROUTER_API_KEY`); you never ask for it, print it, echo it, log it, read it
back, or write it to any file. You only ever confirm it is *set* through `c3 providers`.

The plugin's roster validator knows only `codex`/`agy`/`muse` and refuses the whole file
on any other engine, so an `http` reviewer does **not** go in `reviewers[]`. It goes under
the top-level `ext.c3.reviewers` array, which the plugin validates as an object and
ignores — C3 reads and validates it and appends the reviewer after the plugin's entries:

```json
{
  "roster_version": 1,
  "reviewers": [
    { "provider": "openai", "model": "<the ChatGPT-plan model>", "panel": "weighty" }
  ],
  "ext": { "c3": { "reviewers": [
    {
      "provider": "openrouter",
      "model": "openai/gpt-5",
      "engine": "http",
      "lab": "openai",
      "weight": 1,
      "base_url": "https://openrouter.ai/api/v1",
      "key_env": "OPENROUTER_API_KEY",
      "json_object": true,
      "headers": { "HTTP-Referer": "https://xelth.com", "X-Title": "c3" },
      "purposes": ["diff-review"],
      "roles": ["security"]
    }
  ] } }
}
```

- `base_url` must be `https://` (parsed strictly: a host, no userinfo/query/fragment);
  `key_env` is the *name* of the variable the key is read from (a valid environment-variable
  name), never the key. **c3 sends a variable only to the host it belongs to:** a known key
  (`OPENROUTER_API_KEY`→openrouter.ai, `OPENAI_API_KEY`→api.openai.com, and the Anthropic /
  Gemini / Google / Mistral / DeepSeek / Groq / Together / xAI keys to their hosts) must go to
  that host or a subdomain; any other endpoint needs a variable the user created for c3 whose
  NAME encodes the one host it may be sent to: `C3_KEY_` + the host upper-cased, `.` written `_`
  and `-` written `__` (so `api.example.com` → `C3_KEY_API_EXAMPLE_COM`, `my-llm.internal.example`
  → `C3_KEY_MY__LLM_INTERNAL_EXAMPLE`). A custom key works only for that exact host (no subdomain
  rule; an IP literal cannot use one), and a refusal names the variable to create for the host in
  hand. Anything else is refused — e.g. `OPENAI_API_KEY` with an openrouter base_url, or a
  `C3_KEY_` variable pointed at a host its name does not encode. Header names/values carry no
  CR/LF, must be RFC 7230 tokens, and c3 refuses the ones it controls (`Authorization`, `Cookie`,
  `Host`, `Content-Type`, …) and any name that suggests a credential (`key`, `token`, `secret`,
  `auth`, `password`, `credential`, `session`) — the roster holds no secrets. Refusals report the
  JSON type found, never the value, so a key accidentally pasted into a field is never echoed. `json_object` (default
  `true`) sends `response_format: {"type":"json_object"}`. `pack_tokens` (0..=200000, default
  12000) sizes the pack's periphery for this reviewer; `--pack-budget <n>` overrides per run.
- **Billing guard.** A subscription-engine label (`codex`, `chatgpt`, `muse`, `agy`,
  `antigravity`) is refused outright. A lab label (`openai`, `gemini`, `google`, `meta`) is
  refused unless the entry says `"api_billing": "accepted"` — a user who really wants
  per-token billing at a lab that also sells them a subscription says so once, here. A
  concentrator label like `openrouter` is fine.

**Without a roster** (a one-off), run it directly — the defaults are OpenRouter's:

```
c3 consult --task <t> --brief <brief.md> --artifact <file-to-review> \
  --engine http --provider openrouter --model openai/gpt-5 --dry-run
```

`--base-url <https url>` and `--key-env <NAME>` override the defaults; the key itself is
never a flag. The `http` reviewer receives only the sanitized pack, so the run needs a
`--brief` and at least one `--artifact` (the files to review, shown in full).

## 5. Verify

```
c3 providers
```

Expect exit `0`, a first line `codex config: <path>`, then `endpoint health: <repo>\.collab
(<k> task ledgers, <m> consultations)`, one row per provider, and for each wired one
`available` with `ok: Logged in using ChatGPT` or `ok: env <NAME> set`, e.g.
`available  ZAI  2  custom  https://api.z.ai/api/v1  ok: env ZAI_API_KEY set  zai (11
declared models)  -`, then (with a roster) `roster: <path> -> would select <provider> ::
<model>` and `availability: all <n> reviewers available`. For the one-line view, run
`c3 providers --short`. Other verdicts — the roster walk's own: `unavailable (missing:
env <NAME> not set)` (not set, or Claude Code not restarted), `unavailable (usage limit
until <iso>)`, `unknown (<reason>)` (login check failed, or the config cannot be
scanned). Exit `1` means an unusable roster, or a `CODEX_CONSULT_ROSTER` file that does
not exist; the message names it. Per provider: `c3 providers --provider <name>` (exit `0`
available, `2` unavailable, `3` unknown, `1` no such provider).

Then a dry run inside a git repository, once per reviewer:

```
c3 consult --task setup-check --prompt "Reply with one sentence." --dry-run --provider ZAI --model glm-5.3
```

Expect exit `0`, `DRY RUN - nothing was executed and no file was written.`,
`preflight   : available (ok: env ZAI_API_KEY set)`,
`effort      : high sent (requested high, mapping zai-v1, …)`, and a `transport   :`
line (`output-schema` for openai and z.ai, `prompt-only` for MiMo). Only with the user's
consent, run one live `--purpose chore` consultation to confirm the route end to end
(expect `usable reply - <provider> :: <model>, …`).

## 6. Optional

- **Peak windows:** the user sets `CODEX_CONSULT_PEAK_<PROVIDER>="Mon-Fri 14:00-18:00 +08:00"`
  (days, start-end, fixed offset; schedule from the plan's own page) and optionally
  `CODEX_CONSULT_PEAK_<PROVIDER>_EXCEPT="2026-10-01..2026-10-07"`. A run at peak warns;
  `--off-peak-only` refuses at peak and when no schedule is set.
- **`--schema-transport output-schema|prompt-only`:** a one-run override of caps-v1's
  declared transport, only when you know the declared one is wrong for that endpoint.
- **`"auth": "none"`** in a roster entry: only for a table with neither `env_key` nor a
  bearer token (e.g. a local endpoint); it has no effect on a table that names an
  `env_key`. Such a host is not in caps-v1, so it also needs `--native-effort`.

## 7. Record it

In the project's `state.md` (or its notes file): the providers wired and their models, the
roster path, order and panel weights, the env variable NAMES (never values), any peak
variables, the `c3 providers` verdict per provider with the date, and what is still
unavailable and why (and what the user was asked to do about it).
