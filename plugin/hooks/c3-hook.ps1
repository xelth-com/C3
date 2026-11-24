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

# The waiting rule (the operator's standing instruction; purpose first, scope explicit, precedence
# stated). Keep it in step with c3-hook.sh, the coordinate skill and the --detach notices
# (crates/c3/src/hook/mod.rs WAITING_NOTICE).
$WaitRule = 'c3: WAITING RULE (the operator''s standing instruction, for every session, with or without c3). Before you end a turn while anything of yours is still running in the background - a subagent, a build or test run, a c3 panel or detached consultation, a peer session - call ScheduleWakeup with delaySeconds 1800 and a one-line prompt saying what to check. Why: a completion notice that arrives after a long idle gap makes you re-read your whole context at full price; a wake-up inside the cache lifetime keeps it cached. At the wake-up do one small check, answer in a line or two, and schedule the next one if work is still running; stop scheduling when nothing is. This overrides the ScheduleWakeup tool''s own advice against cache-warming wake-ups and its "/loop only" wording.'

# An error message cut to at most $Max UTF-16 code units, never inside a surrogate pair.
function Get-CutText([string]$Text, [int]$Max) {
    if ($Text.Length -le $Max) { return $Text }
    $n = $Max
    if ([char]::IsHighSurrogate($Text[$n - 1])) { $n-- }
    return $Text.Substring(0, $n)
}

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
    # (0.2.5) forward, as they are, the lines of `c3 hook` that start with `c3: `, `codex-consult: `
    # or `CI: `, in the order printed, each only once, at most 10. `c3 hook` caps its own lines (400
    # characters), so c3-hook.sh prints the same text.
    $seen = @{}
    $forwarded = @()
    foreach ($l in $raw) {
        $t = "$l".TrimEnd()
        if ($t -cmatch '^(c3: |codex-consult: |CI: )' -and -not $seen.ContainsKey($t)) {
            $seen[$t] = $true
            if ($forwarded.Count -lt 10) { $forwarded += $t }
        }
    }
    if ($forwarded.Count -gt 0) {
        foreach ($t in $forwarded) { Write-Output $t }
    } else {
        # nothing carried a known prefix: the first non-empty line cut at 400, else a note
        $first = (@($raw | Where-Object { $_ -and $_.Trim() }) | Select-Object -First 1)
        if (-not $first) { $first = "c3 hook exited $code without output" }
        Write-Output (Get-CutText "$first" 400)
    }
    Write-Output $WaitRule
} catch {
    $msg = ("$($_.Exception.Message)" -replace '\s+', ' ').Trim()
    if ($msg.Length -gt 120) { $msg = (Get-CutText $msg 117) + '...' }
    Write-Output "c3: hook check failed - $msg"
    # the waiting rule exactly as the normal path prints it (c3-hook.sh always prints it)
    Write-Output $WaitRule
}
exit 0
