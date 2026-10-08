# cloud-linux — the first live runs of the `http` engine from Linux, behind a sandbox proxy (2026-10-08)

Purpose: prove the Linux port's `http` engine end to end from a cloud sandbox whose egress goes
through an HTTP CONNECT proxy that attaches the OpenRouter credential itself and re-terminates
TLS with its own CA (`docs/port/linux-port.md` §3). No key is in the environment of these runs;
`C3_HTTP_AUTH_PROXY=openrouter.ai` selects the header-less auth mode and
`C3_HTTP_CA_BUNDLE=/root/.ccr/ca-bundle.crt` adds the proxy's CA to the bundled roots. Money rule:
only models whose prompt and completion prices are 0, plus the `luna` model the owner allowed
(`openai/gpt-6-luna`, 0.10 USD per million input tokens, 0.50 USD per million output tokens).

Two briefs: `brief.md` (the proxy auth mode of the http engine; runs 1-3) and
`handoffs/04-claude-diff-review.md` (the whole branch diff, dogfooding; runs 5-6). Run from the
checkout at `cloud/linux-port`, debug build, `--pack-budget 6000`.

## Consultations

| n | reviewer | brief | result | wall | tokens in / out | mark |
|---|---|---|---|---|---|---|
| 1 | `openrouter :: nvidia/nemotron-3.5-lightning:free [http]` | `brief.md` | failed: transport — `tls connection init failed: invalid peer certificate: UnknownIssuer` (the proxy's CA is not among the bundled roots; `proxy: true`, `auth: proxy`, no `C3_HTTP_CA_BUNDLE` yet) | 0.3 s | - | - |
RUN-ROWS

## What the runs proved

- The request event of run 1 shows the proxy was used (`proxy: true`), the CONNECT tunnel was
  established, and the TLS handshake failed on the issuer: the trust anchors were the missing
  piece, so `C3_HTTP_CA_BUNDLE` was added after run 1 (commit `de24304`).
WHAT-PROVED

## Findings and decisions

FINDINGS
