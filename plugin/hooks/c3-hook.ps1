<#
.SYNOPSIS
    SessionStart hook: one line saying whether c3 and its reviewers are available.

.DESCRIPTION
    Locates the `c3` binary and runs `c3 hook`, which prints one line to stdout
    (added to the agent's context by Claude Code) and never touches the network or
    writes anything beyond reading task ledgers under `<CollabDir>` (default
    `.collab`) - see docs/port/cli-surface.md section 4 in the C3 repository.

    This wrapper always exits 0, even when `c3` is missing or the check itself
    fails: a broken hook must never block a session. When the binary cannot be
    found it prints one line saying so instead of running anything.

    Binary resolution, in this order:
      1. $env:C3_EXE - an explicit override path (used only if it exists).
      2. "$env:CLAUDE_PLUGIN_ROOT/bin/c3(.exe)" - a binary bundled next to this
         plugin, for a future packaged release.
      3. `c3` resolved from PATH.

    Runs on Windows PowerShell 5.1 and PowerShell 7.
#>
[CmdletBinding()]
param(
    # Where consultations are stored (endpoint health is read from every task
    # ledger under it). Relative paths resolve against the git repo root.
    [string]$CollabDir = '.collab'
)

$ErrorActionPreference = 'Continue'

function Resolve-C3Exe {
    if ($env:C3_EXE -and (Test-Path -LiteralPath $env:C3_EXE)) {
        return $env:C3_EXE
    }
    $root = $env:CLAUDE_PLUGIN_ROOT
    if ($root) {
        foreach ($name in @('c3.exe', 'c3')) {
            $candidate = Join-Path $root (Join-Path 'bin' $name)
            if (Test-Path -LiteralPath $candidate) { return $candidate }
        }
    }
    $cmd = Get-Command c3 -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    return $null
}

try {
    $exe = Resolve-C3Exe
    if (-not $exe) {
        Write-Output 'c3: not installed - place the c3 binary on PATH or set C3_EXE (see plugin/README.md)'
        exit 0
    }
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $raw = @(& $exe hook --collab-dir $CollabDir 2>&1 | ForEach-Object { "$_" })
    $code = $LASTEXITCODE
    $ErrorActionPreference = $previous
    $first = (@($raw | Where-Object { $_ -and $_.Trim() }) | Select-Object -First 1)
    if (-not $first) { $first = "c3 hook exited $code without output" }
    Write-Output $first
    # (wave 27, R13 D5) the second line: the pointer to the coordinator's rules (and the telemetry switch)
    $pointer = (@($raw | Where-Object { $_ -like 'codex-consult: coordinator rules - *' }) | Select-Object -First 1)
    if ($pointer -and $pointer -ne $first) { Write-Output $pointer }
} catch {
    $msg = ("$($_.Exception.Message)" -replace '\s+', ' ').Trim()
    if ($msg.Length -gt 120) { $msg = $msg.Substring(0, 117) + '...' }
    Write-Output "c3: hook check failed - $msg"
}
exit 0
