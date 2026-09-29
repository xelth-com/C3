<#
.SYNOPSIS
    Prototype shim: fronts the C3 `c3 hook` binary, reproducing the SessionStart-hook
    contract of `codex-consult-hook.ps1`.

.DESCRIPTION
    See C:\Users\Dmytro\C3\docs\port\cli-surface.md (section 4) for the mapping. Unlike
    the other three shims, the reference `codex-consult-hook.ps1` is not itself a thin
    wrapper the way `codex-providers.ps1` fronts a single subcommand call - it shells
    out to `codex-providers.ps1 -Short -Json -NoNetwork`, parses the JSON `line` field
    out of the merged stdout+stderr text, and ALWAYS exits 0, printing a synthesized
    "reviewer check failed - ..." line instead of ever failing the session. This shim
    reproduces that same always-exit-0, always-print-one-line contract, but delegates
    the actual "what do the reviewers look like" computation to `c3 hook` itself (which
    the C3 binary is expected to implement in-process against its own `providers`
    logic - see cli-surface.md section 4) rather than re-implementing the JSON-line
    extraction dance here.

    If the c3 binary cannot be located at all, or `c3 hook` itself throws, this shim
    still prints a `codex-consult: ...` line and exits 0 - never propagating a
    non-zero exit to the SessionStart hook, matching the reference script's own
    guarantee (a hook that fails would break every Claude Code session in a project
    with this plugin enabled).

.NOTES
    NOT YET RUNNABLE END TO END: `c3 hook` is being implemented in parallel and does
    not exist at the time this shim was written. Validate by parsing only:

      powershell -NoProfile -Command "$errs=$null; [void][System.Management.Automation.Language.Parser]::ParseFile('C:\Users\Dmytro\C3\tests\shim\codex-consult-hook.ps1', [ref]$null, [ref]$errs); if ($errs) { $errs | ForEach-Object { Write-Host $_ } ; exit 1 } else { Write-Host 'parse OK' }"
#>
[CmdletBinding()]
param(
    [string]$CollabDir = '.collab'
)

# (wave 27c, D14) c3 honours a CODEX_CONSULT_TEST_* hook only when CODEX_CONSULT_TEST_MODE is
# set too; the plugin harnesses set the hooks but not (yet) the mode, so this shim sets it when the
# caller did not (a caller-set value wins).
if (-not $env:CODEX_CONSULT_TEST_MODE) { $env:CODEX_CONSULT_TEST_MODE = "1" }


$ErrorActionPreference = 'Stop'
$line = ''

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

try {
    $c3 = Resolve-C3Exe
    if (-not $c3) {
        $line = 'codex-consult: c3 CLI not found (checked $env:C3_EXE, target\debug\c3.exe, and PATH) - follow the setup-providers skill before consulting a reviewer'
    } else {
        $previous = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        $raw = @(& $c3 'hook' '--collab-dir' $CollabDir 2>&1 | ForEach-Object { "$_" })
        $code = $LASTEXITCODE
        $ErrorActionPreference = $previous
        $first = (@($raw | Where-Object { $_ -and $_.Trim() }) | Select-Object -First 1)
        if ($first -match '^codex-consult: ' -and $first -notmatch "[`r`n]") {
            $line = $first
        } else {
            if (-not $first) { $first = "c3 hook exited $code without output" }
            $line = "codex-consult: reviewer check failed - $first"
        }
    }
} catch {
    $msg = ("$($_.Exception.Message)" -replace '\s+', ' ').Trim()
    if ($msg.Length -gt 120) { $msg = $msg.Substring(0, 117) + '...' }
    $line = "codex-consult: reviewer check failed - $msg"
}
Write-Output $line
exit 0
