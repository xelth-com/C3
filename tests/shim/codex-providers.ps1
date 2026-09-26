<#
.SYNOPSIS
    Prototype shim: fronts the C3 `c3 providers` binary with the parameter surface and
    stdout/exit-code contract that the codex-consult plugin's PowerShell harnesses
    (`harness-0.3.ps1`, `harness-engines.ps1`) assert on for `codex-providers.ps1`.

.DESCRIPTION
    See C:\Users\Dmytro\C3\docs\port\harness-shim.md for the full design and
    C:\Users\Dmytro\C3\docs\port\m1-acceptance.md for the acceptance checklist this
    shim is meant to satisfy.

    This is a THIN forwarder: it does no TOML scanning, credential checking or roster
    logic itself. It locates the `c3` binary, maps this script's named parameters onto
    `c3 providers` flags, runs it with stdin/stdout/stderr connected straight through,
    and exits with the binary's own exit code. All of the interesting behaviour
    (verdicts, table/JSON shape, exit codes 0/1/2/3, "wrote nothing") must come from the
    Rust binary; this file is only the seam that lets the existing plugin harnesses
    launch it exactly as they launch the real `codex-providers.ps1`
    (`& $psExe -NoProfile -ExecutionPolicy Bypass -File <this> @ArgList`).

    `c3` binary resolution order:
      1. $env:C3_EXE, if set (explicit override, e.g. from a harness's -Env hashtable).
      2. <repo root>\target\debug\c3.exe, where <repo root> is resolved relative to
         this script's own location (two levels up: tests\shim\.. \.. ), i.e. the
         normal `cargo build` output location for a debug build of this workspace.
      3. `c3` (or `c3.exe`) resolved via PATH.
    If none of these exist, the shim exits 127 with a message on stderr (there is no
    equivalent "unknown provider file" case in the real script's contract, so 127 is a
    deliberately out-of-band code for "the binary itself is missing", not one of the
    provider verdict codes 0/1/2/3).

.PARAMETER Provider
    Forwarded as `c3 providers --provider <name>`.

.PARAMETER CollabDir
    Forwarded as `c3 providers --collab-dir <path>`. Default '.collab', matching the
    real script.

.PARAMETER Json
    Forwarded as `c3 providers --json`.

.PARAMETER CodexExe
    Forwarded as `c3 providers --codex-exe <path>` when given. Also honoured via
    $env:CODEX_CONSULT_EXE (not read here — c3 itself must read that env var the same
    way codex-consult-common.ps1's Resolve-CodexLauncher does, since the shim does not
    duplicate that resolution logic).

.PARAMETER EngineExe
    Forwarded as `c3 providers --engine-exe <path>` when given.

.PARAMETER NoNetwork
    Forwarded as `c3 providers --no-network`.

.PARAMETER Short
    Forwarded as `c3 providers --short`.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File tests\shim\codex-providers.ps1 -Provider ZAI -Json

.EXAMPLE
    # Point at an explicit build:
    $env:C3_EXE = 'C:\Users\Dmytro\C3\target\release\c3.exe'
    powershell -NoProfile -ExecutionPolicy Bypass -File tests\shim\codex-providers.ps1 -Short

.NOTES
    NOT YET RUNNABLE END TO END: `c3 providers` is being implemented in parallel by
    another worker and does not exist at the time this shim was written. Until it does,
    this file can only be validated by parsing it, e.g.:

      powershell -NoProfile -Command "$errs=$null; [void][System.Management.Automation.Language.Parser]::ParseFile('C:\Users\Dmytro\C3\tests\shim\codex-providers.ps1', [ref]$null, [ref]$errs); if ($errs) { $errs | ForEach-Object { Write-Host $_ } ; exit 1 } else { Write-Host 'parse OK' }"

    Once `c3 providers` exists, run this shim directly, or point one of the plugin's
    harnesses at a checkout where this file replaces (or is symlinked over)
    plugins\codex-consult\scripts\codex-providers.ps1 (see docs/port/harness-shim.md,
    section 3, for why the harnesses' hard-coded $scripts path makes that necessary
    today).
#>
[CmdletBinding()]
param(
    [string]$Provider = '',
    [string]$CollabDir = '.collab',
    [switch]$Json,
    [string]$CodexExe = '',
    [string]$EngineExe = '',
    [switch]$NoNetwork,
    [switch]$Short
)

$ErrorActionPreference = 'Stop'

function Resolve-C3Exe {
    if ($env:C3_EXE -and (Test-Path -LiteralPath $env:C3_EXE)) {
        return (Resolve-Path -LiteralPath $env:C3_EXE).Path
    }
    # This script lives at <repoRoot>\tests\shim\codex-providers.ps1.
    $repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
    $debugExe = Join-Path $repoRoot 'target\debug\c3.exe'
    if (Test-Path -LiteralPath $debugExe) {
        return $debugExe
    }
    $onPath = Get-Command 'c3' -ErrorAction SilentlyContinue
    if ($onPath) {
        return $onPath.Source
    }
    return $null
}

$c3 = Resolve-C3Exe
if (-not $c3) {
    Write-Error "codex-providers shim: could not locate the c3 binary (checked `$env:C3_EXE, target\debug\c3.exe relative to the repo root, and PATH)."
    exit 127
}

$c3Args = New-Object System.Collections.Generic.List[string]
$c3Args.Add('providers')

if ($Provider) { $c3Args.Add('--provider'); $c3Args.Add($Provider) }
if ($CollabDir) { $c3Args.Add('--collab-dir'); $c3Args.Add($CollabDir) }
if ($Json) { $c3Args.Add('--json') }
if ($CodexExe) { $c3Args.Add('--codex-exe'); $c3Args.Add($CodexExe) }
if ($EngineExe) { $c3Args.Add('--engine-exe'); $c3Args.Add($EngineExe) }
if ($NoNetwork) { $c3Args.Add('--no-network') }
if ($Short) { $c3Args.Add('--short') }

# Forward stdout/stderr unchanged (no re-encoding, no Write-Host wrapping) so a
# harness's -Json parse (which reads the ENTIRE merged 2>&1 text) sees nothing but
# c3's own output, and so the "wrote nothing" byte-identical check is unaffected by
# anything this shim does.
& $c3 @c3Args
exit $LASTEXITCODE
