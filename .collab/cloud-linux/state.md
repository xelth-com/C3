# cloud-linux — the first live runs of the `http` engine from Linux, behind a sandbox proxy (2026-10-08)

Purpose: prove the Linux port's `http` engine end to end from a cloud sandbox whose egress goes
through an HTTP CONNECT proxy that attaches the OpenRouter credential itself and re-terminates
TLS with its own CA (`docs/port/linux-port.md` §3). No key is in the environment of these runs;
`C3_HTTP_AUTH_PROXY=openrouter.ai` selects the header-less auth mode and
`C3_HTTP_CA_BUNDLE=/root/.ccr/ca-bundle.crt` adds the proxy's CA to the bundled roots. Money rule:
only models whose prompt and completion prices are 0, plus the `luna` model the owner allowed
(`openai/gpt-6-luna`: 0.10 USD per million input tokens, 0.50 USD per million output tokens; one
run of this brief costs about one cent).

Two briefs: `brief.md` (the proxy auth mode of the http engine; runs 1-5) and
`handoffs/04-claude-diff-review.md` (the whole branch diff, dogfooding; runs 6-7). Run from the
checkout at `cloud/linux-port`, debug build, `--pack-budget 6000` (the focus file is sent in
full; the budget is the periphery's). Handoff numbers skip 04, which is the brief.

## Consultations

| n (handoff) | reviewer | brief | result | wall | tokens in / out | mark |
|---|---|---|---|---|---|---|
| 1 (01) | `openrouter :: nvidia/nemotron-3.5-lightning:free [http]` | `brief.md` | failed: transport — `tls connection init failed: invalid peer certificate: UnknownIssuer`: the proxy's CA is not among the bundled roots (`proxy: true`, `auth: proxy`, no `C3_HTTP_CA_BUNDLE` yet) | 0.3 s | - | - |
| 2 (02) | `openrouter :: nvidia/nemotron-3.5-lightning:free [http]` | `brief.md` | failed: the model did not finish within the 600 s read timeout; the engine of that build reported it as an empty `non-JSON response` of class `unknown` (fixed in `f811a0b`: a body read timeout is `TimedOut` and retried once) | 600.0 s | - | - |
| 3 (05) | `openrouter :: google/gemma-4-31b-it:free [http]` | `brief.md` | failed: burst — 429 `Provider returned error` from the free upstream 0.7 s after the request. The proxy, the CA bundle and the header-less auth all worked: a real OpenRouter error envelope came back | 0.7 s | - | - |
| 4 (06) | `openrouter :: nvidia/nemotron-3-super-120b-a12b:free [http]` | `brief.md` | a complete HTTP exchange (200, 117 KB body) but the reply was `{"": ""}`: the model spent 13 814 of its 13 821 output tokens on reasoning and emitted an empty object under JSON mode; recorded INVALID, raw kept, no findings | 131.4 s | 26 405 / 13 821 (13 814 reasoning) | no |
| 5 (07) | `openrouter :: openai/gpt-6-luna [http]` | `brief.md` | usable, HOLD (the schema expects ADVISE for `decision`, recorded as invalid verdict), 5 findings F07-1..F07-5 (4 major, 1 minor); the normaliser repaired five `evidence` objects wrapped in arrays and kept the model's own text in `07-http-reply.original.json` | 183.3 s | 26 405 / 16 961 | yes |
RUN-ROWS

## What the runs proved

- Run 1: the request event shows the proxy was used (`proxy: true`), the CONNECT tunnel was
  established, and the TLS handshake failed on the issuer — the trust anchors were the missing
  piece, so `C3_HTTP_CA_BUNDLE` was added after run 1 (commit `de24304`).
- Run 2: with the proxy's CA trusted the request went through (`ca_bundle: true`) and only the
  model's speed failed it; it exposed the swallowed body-read error (D1).
- Run 3: a real 429 envelope from OpenRouter through the proxy — classed `burst`, not retried,
  as designed.
- Runs 4-5: the whole path works end to end from Linux: pack, request, response, normaliser,
  findings, ledger, events — with no key anywhere in the environment or the files (checked:
  `grep -r "Bearer\|sk-or-" .collab/cloud-linux` finds only the engine's own doc comments inside
  the packed source file).

## Findings and decisions

| id | from | severity | claim | decision |
|---|---|---|---|---|
| F07-1 | luna | major | a bracketed IPv6 `NO_PROXY` entry loses its closing bracket and never matches | accepted — `no_proxy_entry` parses `[v6]` and `[v6]:port`; verified by unit test |
| F07-2 | luna | major | a `NO_PROXY` port qualifier is stripped, so the entry excludes the host on every port | accepted — the request port is passed in; a port-qualified entry excludes only that port |
| F07-3 | luna | major | an IPv4-mapped IPv6 loopback literal (`[::ffff:127.0.0.1]`) is proxied | accepted — `is_loopback_literal` parses the host as an `IpAddr` |
| F07-4 | luna | minor | a Unicode spelling in `C3_HTTP_AUTH_PROXY` does not match the punycode host the URL parser reports | accepted — each list entry is canonicalised with `url::Host::parse` |
| F07-5 | luna | major | the proxy auth mode can be selected while no proxy applies, so the request goes out with no credential | D2: a warning, not a refusal |
| L1 | coordinator (run 2) | major | a body read that times out became an empty reply of class `unknown` | D1 |
| L2 | coordinator (run 4) | minor | a reasoning model under JSON mode may spend its whole budget on reasoning and answer `{"": ""}` | recorded; the engine's handling (INVALID, raw kept) is right; try `--effort low` or a non-reasoning free model for such seats |

- D1. A response body that cannot be read to the end is its own error: the status and elapsed
  time go to the error event; a timeout is `TimedOut` (one retry), anything else `transport`.
- D2. The header-less mode without an applicable proxy is said out loud (`proxy auth without a
  proxy ...` in the attempt's warnings and the ledger) rather than refused: a transparent egress
  proxy that the environment does not name is a real deployment, and nothing is leaked — the
  request goes to the same host, only without a credential, and the provider answers 401.
- D3 (F07's unproven scenario): `env_proxy_applies` and ureq's `try_from_env` read the same
  variables (`ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY`, upper then lower case); ureq picks the
  first set one regardless of scheme, C3 enables it only when the scheme's variable (or
  `ALL_PROXY`) is set, so the one divergence (an `http` URL with only `HTTPS_PROXY` set) ends
  in a direct request, never a surprise proxy.

## Dogfooding: the diff review (runs 6-7)

DOGFOOD
