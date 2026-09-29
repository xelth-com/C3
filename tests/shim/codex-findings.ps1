<#
.SYNOPSIS
    Prototype shim: fronts the C3 `c3 findings` binary with the parameter surface that
    `codex-findings.ps1` exposes to the codex-consult plugin's PowerShell harnesses.

.DESCRIPTION
    See C:\Users\Dmytro\C3\docs\port\cli-surface.md (section 2) for the parameter
    mapping and C:\Users\Dmytro\C3\docs\port\m3-acceptance.md for the acceptance
    checklist. Same thin-forwarder pattern as tests\shim\codex-providers.ps1 and
    tests\shim\codex-consult.ps1: no lock/store logic here, just argv mapping and
    stdout/stderr/exit-code passthrough.

    `-Json` is NOT a parameter of the reference `codex-findings.ps1` (checked the whole
    file - see cli-surface.md Open question 1); it is accepted here anyway as a
    forward-looking C3-only addition and forwarded as `--json` when passed, so this shim
    does not need to change if/when that addition is decided on.

.NOTES
    NOT YET RUNNABLE END TO END: `c3 findings` is being implemented in parallel and does
    not exist at the time this shim was written. Validate by parsing only:

      powershell -NoProfile -Command "$errs=$null; [void][System.Management.Automation.Language.Parser]::ParseFile('C:\Users\Dmytro\C3\tests\shim\codex-findings.ps1', [ref]$null, [ref]$errs); if ($errs) { $errs | ForEach-Object { Write-Host $_ } ; exit 1 } else { Write-Host 'parse OK' }"
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Task,

    [string]$CollabDir = '.collab',
    [switch]$List,
    [switch]$All,
    [switch]$Stats,
    [string]$Id = '',
    [string]$Status = '',
    [string]$Note = '',
    [string]$Evidence = '',
    [int]$Rate = 0,
    [string]$Useful = '',

    # Not a parameter of the reference script (cli-surface.md Open question 1);
    # forwarded as --json when passed, otherwise omitted entirely.
    [switch]$Json
)

# (wave 27c, D14) c3 honours a CODEX_CONSULT_TEST_* hook only when CODEX_CONSULT_TEST_MODE is
# set too; the plugin harnesses set the hooks but not (yet) the mode, so this shim sets it when the
# caller did not (a caller-set value wins).
if (-not $env:CODEX_CONSULT_TEST_MODE) { $env:CODEX_CONSULT_TEST_MODE = "1" }


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
    Write-Error "codex-findings shim: could not locate the c3 binary (checked `$env:C3_EXE, target\debug\c3.exe relative to the repo root, and PATH)."
    exit 127
}

# The harnesses run fake consultations: nothing of them may reach the telemetry hub and no
# priors download may start. A value set by the caller wins.
if (-not $env:CODEX_CONSULT_TELEMETRY) { $env:CODEX_CONSULT_TELEMETRY = "off" }

$c3Args = New-Object System.Collections.Generic.List[string]
$c3Args.Add('findings')

$c3Args.Add('--task'); $c3Args.Add($Task)
if ($CollabDir) { $c3Args.Add('--collab-dir'); $c3Args.Add($CollabDir) }
if ($List) { $c3Args.Add('--list') }
if ($All) { $c3Args.Add('--all') }
if ($Stats) { $c3Args.Add('--stats') }
if ($Id) { $c3Args.Add('--id'); $c3Args.Add($Id) }
if ($Status) { $c3Args.Add('--status'); $c3Args.Add($Status) }
if ($Note) { $c3Args.Add('--note'); $c3Args.Add($Note) }
if ($Evidence) { $c3Args.Add('--evidence'); $c3Args.Add($Evidence) }
if ($Rate -ne 0) { $c3Args.Add('--rate'); $c3Args.Add([string]$Rate) }
if ($Useful) { $c3Args.Add('--useful'); $c3Args.Add($Useful) }
if ($Json) { $c3Args.Add('--json') }

& $c3 @c3Args
exit $LASTEXITCODE
