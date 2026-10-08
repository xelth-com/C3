<#
.SYNOPSIS
    Shim (wave 2c, F02-4): fronts `c3 telemetry` / `c3 complain` with the parameter surface of the
    plugin's `codex-telemetry.ps1` (v0.6.1), so the plugin's PowerShell harnesses run C3's
    telemetry instead of the plugin's.

.DESCRIPTION
    Same thin-forwarder pattern as the other shims in this directory: no telemetry logic here, only
    argv mapping and stdout/stderr/exit-code passthrough.

      -Flush [-Telemetry on|off]                      -> c3 telemetry --flush [--telemetry ..]
      -Status                                         -> c3 telemetry --status
      -Forget [-PublicRef <ref>] [-Local] [-Yes]      -> c3 telemetry --forget [--public-ref ..] [--local] [--yes]
      -BackfillRatings [-DryRun] [-CollabDir <dir>]
                       [-Telemetry on|off]            -> c3 telemetry --backfill-ratings [--dry-run] [--collab-dir ..] [--telemetry ..]
      -Complain "<text>" [-Yes] [-CollabDir <dir>]    -> c3 complain "<text>" [--yes] [--collab-dir ..]
                                                         (-Contact and -Task: C3's complaint takes
                                                         neither - its context is the newest task's
                                                         last entry; they are accepted and dropped)
      no form / -Local or -PublicRef without -Forget / -DryRun without -BackfillRatings
                                                      -> forwarded so c3 refuses them with the
                                                         plugin's wording (exit 1)

    C3 keeps its OWN telemetry files under <codex home>/c3/telemetry/ (its own app id `c3`, salt,
    instance id and outbox - docs/port/wave2-telemetry.md): checks that read the plugin's
    <codex home>/telemetry-spool/ or <codex home>/telemetry-salt see none of C3's files.

    Safety nets (a harness never reaches a real intake or the priors hub): when neither
    CODEX_CONSULT_TELEMETRY_URL nor C3_TELEMETRY_HUB is set, the intake is pointed at a closed
    loopback port (http://127.0.0.1:9/, the plugin's run-all default); C3_PRIORS=off unless the
    caller set it. The telemetry switch itself is NOT forced (-Status must report the caller's).

.NOTES
    Validate by parsing:
      powershell -NoProfile -Command "$errs=$null; [void][System.Management.Automation.Language.Parser]::ParseFile('<path>\codex-telemetry.ps1', [ref]$null, [ref]$errs); if ($errs) { $errs | ForEach-Object { Write-Host $_ }; exit 1 } else { Write-Host 'parse OK' }"
#>
[CmdletBinding()]
param(
    [switch]$Flush,
    [switch]$Status,
    [string]$Complain = '',
    [string]$Contact = '',
    [switch]$Yes,
    [string]$Task = '',
    [string]$CollabDir = '.collab',
    [string]$Telemetry = '',
    [switch]$BackfillRatings,
    [switch]$DryRun,
    [switch]$Forget,
    [string]$PublicRef = '',
    [switch]$Local
)

$ErrorActionPreference = 'Stop'

function Resolve-C3Exe {
    if ($env:C3_EXE -and (Test-Path -LiteralPath $env:C3_EXE)) {
        return (Resolve-Path -LiteralPath $env:C3_EXE).Path
    }
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
    Write-Error "codex-telemetry shim: could not locate the c3 binary (checked `$env:C3_EXE, target\debug\c3.exe relative to the repo root, and PATH)."
    exit 127
}

if (-not $env:CODEX_CONSULT_TELEMETRY_URL -and -not $env:C3_TELEMETRY_HUB) { $env:CODEX_CONSULT_TELEMETRY_URL = 'http://127.0.0.1:9/' }
if (-not $env:C3_PRIORS) { $env:C3_PRIORS = 'off' }

$c3Args = New-Object System.Collections.Generic.List[string]
if ($PSBoundParameters.ContainsKey('Complain')) {
    $c3Args.Add('complain')
    $c3Args.Add($Complain)
    if ($Yes) { $c3Args.Add('--yes') }
    if ($PSBoundParameters.ContainsKey('CollabDir')) { $c3Args.Add('--collab-dir'); $c3Args.Add($CollabDir) }
} else {
    $c3Args.Add('telemetry')
    if ($Flush) { $c3Args.Add('--flush') }
    if ($Status) { $c3Args.Add('--status') }
    if ($Forget) { $c3Args.Add('--forget') }
    if ($BackfillRatings) { $c3Args.Add('--backfill-ratings') }
    if ($DryRun) { $c3Args.Add('--dry-run') }
    if ($PublicRef) { $c3Args.Add('--public-ref'); $c3Args.Add($PublicRef) }
    if ($Local) { $c3Args.Add('--local') }
    if ($Yes) { $c3Args.Add('--yes') }
    if ($PSBoundParameters.ContainsKey('CollabDir')) { $c3Args.Add('--collab-dir'); $c3Args.Add($CollabDir) }
    if ($Telemetry) { $c3Args.Add('--telemetry'); $c3Args.Add($Telemetry) }
}

& $c3 @c3Args
exit $LASTEXITCODE
