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

$WaitRule = 'c3: coordinator rules - the c3:coordinate skill; while waiting on workers, panels, builds or peers schedule a wakeup (ScheduleWakeup, about 20 min) instead of blocking, small reads at each wake - the prompt cache stays warm'

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
    # (R30 wave-boundary compaction) the SessionStart source arrives as JSON on stdin; read it
    # only when input is redirected (never block an interactive shell) and pass it to `c3 hook`.
    # Only `compact` changes the output, so a parse failure (source stays empty) is harmless.
    $source = ''
    try {
        if ([Console]::IsInputRedirected) {
            $stdin = [Console]::In.ReadToEnd()
            if ($stdin) {
                $evt = $stdin | ConvertFrom-Json -ErrorAction Stop
                if ($evt.source) { $source = [string]$evt.source }
            }
        }
    } catch { $source = '' }
    $exe = Resolve-C3Exe
    if (-not $exe) {
        Write-Output 'c3: not installed - place the c3 binary on PATH or set C3_EXE (see plugin/README.md)'
        Write-Output $WaitRule
        exit 0
    }
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $raw = @(& $exe hook --collab-dir $CollabDir --source $source 2>&1 | ForEach-Object { "$_" })
    $code = $LASTEXITCODE
    $ErrorActionPreference = $previous
    $first = (@($raw | Where-Object { $_ -and $_.Trim() }) | Select-Object -First 1)
    if (-not $first) { $first = "c3 hook exited $code without output" }
    Write-Output $first
    # (wave 27, R13 D5) the second line: the pointer to the coordinator's rules (and the telemetry switch)
    $pointer = (@($raw | Where-Object { $_ -like 'codex-consult: coordinator rules - *' }) | Select-Object -First 1)
    if ($pointer -and $pointer -ne $first) { Write-Output $pointer }
    # (R30) after a compaction, the coordinator re-hydration line (present only for the compact source)
    $restore = (@($raw | Where-Object { $_ -like 'codex-consult: after compaction - *' }) | Select-Object -First 1)
    if ($restore) { Write-Output $restore }
    # (wave 11) the self-update line: present only when `c3 hook` found a newer release on record
    $update = (@($raw | Where-Object { $_ -like 'c3: update available *' }) | Select-Object -First 1)
    if ($update -and $update -ne $first) { Write-Output $update }
    Write-Output $WaitRule
} catch {
    $msg = ("$($_.Exception.Message)" -replace '\s+', ' ').Trim()
    if ($msg.Length -gt 120) { $msg = $msg.Substring(0, 117) + '...' }
    Write-Output "c3: hook check failed - $msg"
}
exit 0
