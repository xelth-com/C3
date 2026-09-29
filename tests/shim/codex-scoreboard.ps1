<#
.SYNOPSIS
    Prototype shim: fronts the C3 `c3 scoreboard` binary with the parameter surface
    that `codex-scoreboard.ps1` exposes to the codex-consult plugin's PowerShell
    harnesses.

.DESCRIPTION
    See C:\Users\Dmytro\C3\docs\port\cli-surface.md (section 3) for the parameter
    mapping and C:\Users\Dmytro\C3\docs\port\m3-acceptance.md for the acceptance
    checklist. Same thin-forwarder pattern as the other tests\shim\*.ps1 files. `c3
    scoreboard` is a NEW subcommand on the `c3` CLI surface (the plugin's
    codex-scoreboard.ps1 has no `c3` equivalent yet as of this writing) - this shim is
    written against the proposed mapping in cli-surface.md, not against an existing
    `c3 scoreboard --help`.

.NOTES
    NOT YET RUNNABLE END TO END: `c3 scoreboard` is being implemented in parallel and
    does not exist at the time this shim was written. Validate by parsing only:

      powershell -NoProfile -Command "$errs=$null; [void][System.Management.Automation.Language.Parser]::ParseFile('C:\Users\Dmytro\C3\tests\shim\codex-scoreboard.ps1', [ref]$null, [ref]$errs); if ($errs) { $errs | ForEach-Object { Write-Host $_ } ; exit 1 } else { Write-Host 'parse OK' }"
#>
[CmdletBinding()]
param(
    [string]$CollabDir = '.collab',
    [string]$Task = '',
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
    Write-Error "codex-scoreboard shim: could not locate the c3 binary (checked `$env:C3_EXE, target\debug\c3.exe relative to the repo root, and PATH)."
    exit 127
}

$c3Args = New-Object System.Collections.Generic.List[string]
$c3Args.Add('scoreboard')

if ($CollabDir) { $c3Args.Add('--collab-dir'); $c3Args.Add($CollabDir) }
if ($Task) { $c3Args.Add('--task'); $c3Args.Add($Task) }
if ($Json) { $c3Args.Add('--json') }

& $c3 @c3Args
exit $LASTEXITCODE
