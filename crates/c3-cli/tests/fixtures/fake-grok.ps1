# FAKE grok (Grok Build CLI headless) for the c3 grok-engine harness - never calls any model and
# never starts the real CLI. Started through fake-grok.cmd, which hands the command line over as
# FAKE_GROK_ARGS (parsed here by the C runtime rules the bridge quotes by). The prompt arrives
# through --prompt-file (stdin stays empty, like muse). Prints the events of
# `grok --output-format streaming-json`: text {data} (the reply, split across chunks), usage
# {usage}, and the ONE end {stopReason, sessionId, requestId, cwd, usage}; an error {message} on a
# start failure.
#   FAKE_GROK_ARGV_LOG=<path>   EVERY start appends one JSON line: {kind (turn | version | other),
#                               argv, prompt, prompt_len, cwd, resume, env (the variable NAMES,
#                               sorted)} - the harness reads it (the env NAMES prove the billing
#                               guard stripped the xAI API key).
#   `--version`                 "grok 1.0.70-fake"
#   turn (--prompt-file):
#   FAKE_GROK_REPLY=<file>      the reply text emitted as text events (first turn)
#   FAKE_GROK_RESUME_REPLY=<file>  on a --resume turn (a format-repair / continuation): this instead
#   FAKE_GROK_SESSION=other     the end names a new session (a resume landed elsewhere);
#                               =notuuid: a non-uuid session id
#   FAKE_GROK_ERROR=<text>      an error event with that message; exit 1 (no end), a start failure
#   FAKE_GROK_STOP=<reason>     the end's stopReason (default end_turn; e.g. max_turn_requests)
#   FAKE_GROK_EXIT=<n>          exits n after the events
#   FAKE_GROK_STDERR=<text>     written to stderr
$ErrorActionPreference = 'Stop'
$u8 = New-Object System.Text.UTF8Encoding($false)
$raw = [string]$env:FAKE_GROK_ARGS
function Out-Bytes { param([string]$Text) $b = $u8.GetBytes($Text); $s = [Console]::OpenStandardOutput(); $s.Write($b, 0, $b.Length); $s.Flush() }
function Err-Bytes { param([string]$Text) $b = $u8.GetBytes($Text + "`r`n"); $s = [Console]::OpenStandardError(); $s.Write($b, 0, $b.Length); $s.Flush() }
function J { param($Obj) return (ConvertTo-Json -InputObject $Obj -Compress -Depth 30) }
function Invoke-FakeWrite { param([scriptblock]$Do) for ($i = 0; $i -lt 60; $i++) { try { & $Do; return } catch { Start-Sleep -Milliseconds 50 } }; & $Do }

# the C runtime's argv rules (backslashes, \", quotes) - what grok.exe reads
function Split-CrtArgs {
    param([string]$Line)
    $out = New-Object System.Collections.Generic.List[string]
    $cur = New-Object System.Text.StringBuilder
    $inQ = $false; $have = $false; $i = 0
    while ($i -lt $Line.Length) {
        $c = $Line[$i]
        if ($c -eq [char]'\') {
            $n = 0
            while ($i -lt $Line.Length -and $Line[$i] -eq [char]'\') { $n++; $i++ }
            if ($i -lt $Line.Length -and $Line[$i] -eq [char]'"') {
                [void]$cur.Append('\', [int][Math]::Floor($n / 2))
                if ($n % 2 -eq 1) { [void]$cur.Append('"'); $i++ } else { $inQ = -not $inQ; $i++ }
            } else { [void]$cur.Append('\', $n) }
            $have = $true
            continue
        }
        if ($c -eq [char]'"') {
            if ($inQ -and $i + 1 -lt $Line.Length -and $Line[$i + 1] -eq [char]'"') { [void]$cur.Append('"'); $i += 2; $have = $true; continue }
            $inQ = -not $inQ; $i++; $have = $true; continue
        }
        if (-not $inQ -and ($c -eq [char]' ' -or $c -eq [char]"`t")) {
            if ($have) { $out.Add($cur.ToString()); [void]$cur.Clear(); $have = $false }
            $i++; continue
        }
        [void]$cur.Append($c); $have = $true; $i++
    }
    if ($have) { $out.Add($cur.ToString()) }
    return , ([string[]]$out.ToArray())
}
$argv = Split-CrtArgs $raw
function Get-Flag { param([string]$Name) for ($k = 0; $k -lt $argv.Count - 1; $k++) { if ($argv[$k] -ceq $Name) { return $argv[$k + 1] } }; return '' }
function Test-Flag { param([string]$Name) return ($argv -ccontains $Name) }

$kind = 'other'
if ($argv -ccontains '--version') { $kind = 'version' }
elseif ($argv -ccontains '--prompt-file') { $kind = 'turn' }

$resume = Get-Flag '--resume'
$promptFile = Get-Flag '--prompt-file'
$prompt = ''
if ($promptFile -and (Test-Path $promptFile)) { $prompt = [IO.File]::ReadAllText($promptFile, $u8) }

if ($env:FAKE_GROK_ARGV_LOG) {
    $names = @([Environment]::GetEnvironmentVariables().Keys | ForEach-Object { [string]$_ } | Where-Object { $_ -ne 'FAKE_GROK_ARGS' })
    $names = [string[]]$names
    [Array]::Sort($names, [StringComparer]::OrdinalIgnoreCase)
    $rec = [pscustomobject]@{ kind = $kind; argv = [object[]]$argv; prompt = $prompt; prompt_len = $prompt.Length; cwd = (Get-Location).Path; resume = [string]$resume; env = [object[]]$names; pid = $PID }
    $line = (J $rec) + "`n"
    Invoke-FakeWrite { [IO.File]::AppendAllText($env:FAKE_GROK_ARGV_LOG, $line, $u8) }
}

if ($kind -eq 'version') { Out-Bytes "grok 1.0.70-fake`n"; exit 0 }
if ($kind -ne 'turn') { Err-Bytes "fake grok: unknown invocation: $raw"; exit 2 }

if ($env:FAKE_GROK_STDERR) { Err-Bytes $env:FAKE_GROK_STDERR }

# a start failure: an error event, no end
if ($env:FAKE_GROK_ERROR) {
    Out-Bytes ((J ([pscustomobject]@{ type = 'error'; message = [string]$env:FAKE_GROK_ERROR; cwd = (Get-Location).Path })) + "`n")
    exit 1
}

# the session id: grok echoes a --resume id, else mints its own (printed only in the end event).
$id = if ($resume) { $resume } else { [guid]::NewGuid().ToString() }
$smode = [string]$env:FAKE_GROK_SESSION
if ($smode -eq 'other') { $id = [guid]::NewGuid().ToString() }
if ($smode -eq 'notuuid') { $id = 'session-1' }

# the reply: the resume reply on a --resume turn, else the first-turn reply.
$replyFile = $env:FAKE_GROK_REPLY
if ($resume -and $env:FAKE_GROK_RESUME_REPLY) { $replyFile = $env:FAKE_GROK_RESUME_REPLY }
$reply = ''
if ($replyFile -and (Test-Path $replyFile)) { $reply = [IO.File]::ReadAllText($replyFile, $u8) }

# an optional thought, then the reply split across two text events (the bridge concatenates them).
Out-Bytes ((J ([pscustomobject]@{ type = 'thought'; data = 'thinking about it' })) + "`n")
if ($reply.Length -gt 1) {
    $mid = [int][Math]::Floor($reply.Length / 2)
    Out-Bytes ((J ([pscustomobject]@{ type = 'text'; data = $reply.Substring(0, $mid) })) + "`n")
    Out-Bytes ((J ([pscustomobject]@{ type = 'text'; data = $reply.Substring($mid) })) + "`n")
} elseif ($reply.Length -eq 1) {
    Out-Bytes ((J ([pscustomobject]@{ type = 'text'; data = $reply })) + "`n")
}

$usage = [pscustomobject]@{ input_tokens = 5820; output_tokens = 83; cache_read_input_tokens = 3840; cache_creation_input_tokens = 0; reasoning_tokens = 82 }
Out-Bytes ((J ([pscustomobject]@{ type = 'usage'; usage = $usage; signature = 'FAKESIG' })) + "`n")

$stop = if ($env:FAKE_GROK_STOP) { [string]$env:FAKE_GROK_STOP } else { 'end_turn' }
$endUsage = [pscustomobject]@{ input_tokens = 5820; cache_read_input_tokens = 3840; cache_creation_input_tokens = 0; output_tokens = 83; reasoning_tokens = 82; total_tokens = 9743 }
Out-Bytes ((J ([pscustomobject]@{ type = 'end'; stopReason = $stop; sessionId = $id; requestId = [guid]::NewGuid().ToString(); cwd = (Get-Location).Path; usage = $endUsage; num_turns = 1; total_cost_usd = 0.014 })) + "`n")

if ($env:FAKE_GROK_EXIT) { exit ([int]$env:FAKE_GROK_EXIT) }
exit 0
