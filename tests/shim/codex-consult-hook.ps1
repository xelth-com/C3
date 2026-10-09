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
    guarantee (a hook that fails would break the coordinator's session in a project
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
$pointer = ''

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

# (wave 2b) Windows PowerShell 5.1 (and pwsh before 7.3, or $PSNativeCommandArgumentPassing Legacy)
# hands a native command its arguments without escaping an embedded quote - it only wraps an
# argument that holds whitespace in quotes - so `a "b" c` reached c3 as `a b c` (engines RUN A8,
# the hook's pointer command). Each argument is pre-escaped by the C runtime's rules (a quote as \",
# the backslashes before it doubled; trailing backslashes doubled when the host wraps the argument),
# so c3 receives it exactly as it was given. A newer host escapes by itself: nothing is changed.
function Get-NativeArgs {
    param([string[]]$List)
    $v = $PSVersionTable.PSVersion
    $legacy = ($v.Major -lt 7) -or ($v.Major -eq 7 -and $v.Minor -lt 3) -or ((Get-Variable -Name PSNativeCommandArgumentPassing -ValueOnly -ErrorAction SilentlyContinue) -eq 'Legacy')
    if (-not $legacy) { return , ([string[]]$List) }
    $out = foreach ($a in $List) {
        $ws = $a -match '\s'
        if ($a -notmatch '"' -and -not ($ws -and $a.EndsWith('\'))) { $a; continue }
        $sb = New-Object System.Text.StringBuilder
        $bs = 0
        foreach ($ch in $a.ToCharArray()) {
            if ($ch -eq [char]'\') { $bs++; continue }
            if ($ch -eq [char]'"') { [void]$sb.Append('\', 2 * $bs + 1); [void]$sb.Append('"'); $bs = 0; continue }
            if ($bs -gt 0) { [void]$sb.Append('\', $bs); $bs = 0 }
            [void]$sb.Append($ch)
        }
        if ($bs -gt 0) { [void]$sb.Append('\', $(if ($ws) { 2 * $bs } else { $bs })) }
        $sb.ToString()
    }
    return , ([string[]]$out)
}

try {
    $c3 = Resolve-C3Exe
    if (-not $c3) {
        $line = 'codex-consult: c3 CLI not found (checked $env:C3_EXE, target\debug\c3.exe, and PATH) - follow the setup-providers skill before consulting a reviewer'
    } else {
        $previous = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        # (wave 27c, D13) the pointer line names THIS plugin's script, runnable as written
        $explainScript = Join-Path $PSScriptRoot 'codex-consult.ps1'
        $explainCmd = $(if ($null -eq $IsWindows -or $IsWindows) { "powershell -NoProfile -ExecutionPolicy Bypass -File ""$explainScript"" -Explain coordinate" } else { "pwsh -NoProfile -File ""$explainScript"" -Explain coordinate" })
        $hookArgs = Get-NativeArgs @('hook', '--collab-dir', $CollabDir, '--explain-command', $explainCmd)
        $raw = @(& $c3 @hookArgs 2>&1 | ForEach-Object { "$_" })
        $code = $LASTEXITCODE
        $ErrorActionPreference = $previous
        $first = (@($raw | Where-Object { $_ -and $_.Trim() }) | Select-Object -First 1)
        # (wave 27, R13 D5) the second line: the pointer to the coordinator's rules
        $pointer = (@($raw | Where-Object { $_ -like 'codex-consult: coordinator rules - *' }) | Select-Object -First 1)
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
if ($pointer -and $pointer -ne $line) { Write-Output $pointer }
exit 0
