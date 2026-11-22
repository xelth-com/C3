# FAKE kimi (Moonshot Kimi CLI headless) for the c3 kimi-engine harness - never calls any model and
# never starts the real CLI. Started through fake-kimi.cmd, which hands the command line over as
# FAKE_KIMI_ARGS (parsed here by the C runtime rules the bridge quotes by). The prompt arrives inline
# in argv (`-p <prompt>`). Prints the role-based events of `kimi -p ... --output-format stream-json`:
# an optional assistant tool-call line, the final assistant {content} line (the reply), and the ONE
# meta {session.resume_hint, session_id}; an `error:` line on stderr + exit 1 on a start failure.
#   FAKE_KIMI_ARGV_LOG=<path>   EVERY start appends one JSON line: {kind (turn | version | other),
#                               argv, prompt, prompt_len, cwd, resume, env (NAMES, sorted), pid,
#                               prompt_body (the assembled prompt the argv pointer names)}.
#   `--version` / `-V`          "kimi 0.27.0-fake"
#   turn (-p):
#   FAKE_KIMI_REPLY=<file>      the reply text emitted as the final assistant content (first turn)
#   FAKE_KIMI_RESUME_REPLY=<file>  on a `-S` turn (a continuation): this instead
#   FAKE_KIMI_TOOLCALL=1        also emit a leading assistant tool-call line (tool_calls > 0)
#   FAKE_KIMI_RESUME_TOOLCALL=<n>  on a `-S` turn (a read-insist re-ask): emit an assistant line
#                               carrying n tool_calls (proving the re-ask read the code); 0/unset = none
#   FAKE_KIMI_SESSION=other     the resume hint names a new session (a resume landed elsewhere);
#                               =notsession: a non-`session_<uuid>` id
#   FAKE_KIMI_ERROR=<text>      an `error: <text>` line on stderr; exit 1 (no resume hint)
#   FAKE_KIMI_EXIT=<n>          exits n after the events
#   FAKE_KIMI_STDERR=<text>     written to stderr
#   FAKE_KIMI_JUNCTION=<dir>    leave a `node_modules` junction to <dir> in the cwd (the throwaway
#                               worktree), as a reviewer's install command could (F02-21)
#   FAKE_KIMI_WRITE=<rel>       write that file (relative to the cwd, the throwaway worktree): the
#                               reviewer's own write (B1)
#   FAKE_KIMI_TOUCH=<abs path>  write that file (an absolute path - the live checkout): a parallel
#                               session changing the main tree during the run (B1)
#   (B1) the log line also carries `head`: the commit the cwd checks out (read from its `.git` file
#   / directory, no git needed), and `ls`: the cwd's top-level names.
$ErrorActionPreference = 'Stop'
$u8 = New-Object System.Text.UTF8Encoding($false)
$raw = [string]$env:FAKE_KIMI_ARGS
function Out-Bytes { param([string]$Text) $b = $u8.GetBytes($Text); $s = [Console]::OpenStandardOutput(); $s.Write($b, 0, $b.Length); $s.Flush() }
function Err-Bytes { param([string]$Text) $b = $u8.GetBytes($Text + "`r`n"); $s = [Console]::OpenStandardError(); $s.Write($b, 0, $b.Length); $s.Flush() }
function J { param($Obj) return (ConvertTo-Json -InputObject $Obj -Compress -Depth 30) }
function Invoke-FakeWrite { param([scriptblock]$Do) for ($i = 0; $i -lt 60; $i++) { try { & $Do; return } catch { Start-Sleep -Milliseconds 50 } }; & $Do }

# the C runtime's argv rules (backslashes, \", quotes) - what kimi.exe reads
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

$kind = 'other'
if (($argv -ccontains '--version') -or ($argv -ccontains '-V')) { $kind = 'version' }
elseif ($argv -ccontains '-p') { $kind = 'turn' }

$resume = Get-Flag '-S'
$prompt = Get-Flag '-p'

# (B) the argv prompt is a POINTER: "Your brief is the file <abs>. Read it ...". Extract the path
# and check it exists AT RUN TIME (the orchestrator removes the prompt file after the run).
$brief = ''
$m = [regex]::Match($prompt, 'Your brief is the file (.+?)\. Read it')
if ($m.Success) { $brief = $m.Groups[1].Value }
$briefExists = ($brief -ne '') -and (Test-Path -LiteralPath $brief -PathType Leaf)
# (R7) the assembled prompt (the pointer target) names the OPERATOR brief by a repo-relative path;
# resolve it against THIS cwd (the worktree) and record whether it is present there, and its contents.
$opBrief = ''
$opBriefExists = $false
$opBriefBody = ''
$promptBody = ''
if ($briefExists) {
    try {
        $body = [IO.File]::ReadAllText($brief, $u8)
        $promptBody = $body
        $bm = [regex]::Match($body, 'Read the brief at `([^`]+)`')
        if ($bm.Success) {
            $opBrief = $bm.Groups[1].Value
            $full = if ([IO.Path]::IsPathRooted($opBrief)) { $opBrief } else { Join-Path (Get-Location).Path $opBrief }
            $opBriefExists = Test-Path -LiteralPath $full -PathType Leaf
            if ($opBriefExists) { $opBriefBody = [IO.File]::ReadAllText($full, $u8) }
        }
    } catch {}
}
# (F11-3) When FAKE_KIMI_OP_BRIEF names a repo-relative brief, resolve it against THIS cwd on EVERY
# turn (primary AND read-insist re-ask) and record its presence + contents. A read-insist re-ask runs
# in a NEW HEAD worktree, so the operator's brief must have been staged there too.
if ($env:FAKE_KIMI_OP_BRIEF) {
    $opBrief = [string]$env:FAKE_KIMI_OP_BRIEF
    $full = if ([IO.Path]::IsPathRooted($opBrief)) { $opBrief } else { Join-Path (Get-Location).Path $opBrief }
    $opBriefExists = Test-Path -LiteralPath $full -PathType Leaf
    $opBriefBody = ''
    if ($opBriefExists) { try { $opBriefBody = [IO.File]::ReadAllText($full, $u8) } catch {} }
}
# (A) a linked git worktree has a `.git` FILE (not a directory) in its root.
$gitFile = Test-Path -LiteralPath (Join-Path (Get-Location).Path '.git') -PathType Leaf
# (B1) the commit the cwd checks out: a detached worktree's HEAD file holds the sha itself.
function Get-CwdHead {
    try {
        $gf = Join-Path (Get-Location).Path '.git'
        $gd = $gf
        if (Test-Path -LiteralPath $gf -PathType Leaf) {
            $line = ([IO.File]::ReadAllText($gf)).Trim()
            if ($line -notmatch '^gitdir:\s*(.+)$') { return '' }
            $gd = $Matches[1].Trim()
            if (-not [IO.Path]::IsPathRooted($gd)) { $gd = Join-Path (Get-Location).Path $gd }
        } elseif (-not (Test-Path -LiteralPath $gf -PathType Container)) { return '' }
        return ([IO.File]::ReadAllText((Join-Path $gd 'HEAD'))).Trim()
    } catch { return '' }
}
$cwdHead = Get-CwdHead
$cwdLs = @(Get-ChildItem -LiteralPath (Get-Location).Path -Force -Name | ForEach-Object { [string]$_ } | Sort-Object)

if ($env:FAKE_KIMI_ARGV_LOG) {
    $names = @([Environment]::GetEnvironmentVariables().Keys | ForEach-Object { [string]$_ } | Where-Object { $_ -ne 'FAKE_KIMI_ARGS' })
    $names = [string[]]$names
    [Array]::Sort($names, [StringComparer]::OrdinalIgnoreCase)
    $rec = [pscustomobject]@{ kind = $kind; argv = [object[]]$argv; prompt = $prompt; prompt_len = $prompt.Length; cwd = (Get-Location).Path; resume = [string]$resume; brief = $brief; brief_exists = $briefExists; op_brief = $opBrief; op_brief_exists = $opBriefExists; op_brief_body = $opBriefBody; prompt_body = $promptBody; git_file = $gitFile; head = $cwdHead; ls = [object[]]$cwdLs; env = [object[]]$names; pid = $PID }
    $line = (J $rec) + "`n"
    Invoke-FakeWrite { [IO.File]::AppendAllText($env:FAKE_KIMI_ARGV_LOG, $line, $u8) }
}

if ($kind -eq 'version') { Out-Bytes "kimi 0.27.0-fake`n"; exit 0 }
if ($kind -ne 'turn') { Err-Bytes "fake kimi: unknown invocation: $raw"; exit 2 }

if ($env:FAKE_KIMI_STDERR) { Err-Bytes $env:FAKE_KIMI_STDERR }
if ($env:FAKE_KIMI_JUNCTION) { $null = New-Item -ItemType Junction -Path (Join-Path (Get-Location).Path 'node_modules') -Target ([string]$env:FAKE_KIMI_JUNCTION) }
if ($env:FAKE_KIMI_WRITE) { [IO.File]::WriteAllText((Join-Path (Get-Location).Path ([string]$env:FAKE_KIMI_WRITE)), "written by the reviewer`n", $u8) }
if ($env:FAKE_KIMI_TOUCH) { [IO.File]::WriteAllText([string]$env:FAKE_KIMI_TOUCH, "a parallel session's edit`n", $u8) }

# a start failure: kimi prints the reason on stderr and exits non-zero, with NO resume hint.
if ($env:FAKE_KIMI_ERROR) {
    Err-Bytes ("error: failed to run prompt: " + [string]$env:FAKE_KIMI_ERROR)
    exit 1
}

# the session id: kimi echoes a `-S` id, else mints its own `session_<uuid>`.
$id = if ($resume) { $resume } else { 'session_' + [guid]::NewGuid().ToString() }
$smode = [string]$env:FAKE_KIMI_SESSION
if ($smode -eq 'other') { $id = 'session_' + [guid]::NewGuid().ToString() }
if ($smode -eq 'notsession') { $id = 'sess-1' }

# the reply: the resume reply on a `-S` turn, else the first-turn reply.
$replyFile = $env:FAKE_KIMI_REPLY
if ($resume -and $env:FAKE_KIMI_RESUME_REPLY) { $replyFile = $env:FAKE_KIMI_RESUME_REPLY }
$reply = ''
if ($replyFile -and (Test-Path $replyFile)) { $reply = [IO.File]::ReadAllText($replyFile, $u8) }

# an optional tool-call line (narration + a Read call), proving the model read the code.
if ($env:FAKE_KIMI_TOOLCALL) {
    $tc = [pscustomobject]@{ type = 'function'; id = 'tool_fakeread'; function = [pscustomobject]@{ name = 'Read'; arguments = '{"path":"app.txt"}' } }
    Out-Bytes ((J ([pscustomobject]@{ role = 'assistant'; content = 'Reading app.txt.'; tool_calls = @($tc) })) + "`n")
    Out-Bytes ((J ([pscustomobject]@{ role = 'tool'; tool_call_id = 'tool_fakeread'; content = "1`tone" })) + "`n")
}

# read-insist: on a `-S` (resume) turn emit an assistant line carrying N tool_calls (a Read each),
# so the re-ask's tool_calls count is N - the evidence the model read the code this time.
$rtc = 0
if ($resume -and $env:FAKE_KIMI_RESUME_TOOLCALL) { $rtc = [int]$env:FAKE_KIMI_RESUME_TOOLCALL }
if ($rtc -gt 0) {
    $calls = @()
    for ($j = 0; $j -lt $rtc; $j++) {
        $calls += [pscustomobject]@{ type = 'function'; id = "tool_insist_$j"; function = [pscustomobject]@{ name = 'Read'; arguments = '{"path":"app.txt"}' } }
    }
    Out-Bytes ((J ([pscustomobject]@{ role = 'assistant'; content = 'Reading the changed files.'; tool_calls = @($calls) })) + "`n")
    for ($j = 0; $j -lt $rtc; $j++) {
        Out-Bytes ((J ([pscustomobject]@{ role = 'tool'; tool_call_id = "tool_insist_$j"; content = "1`tone" })) + "`n")
    }
}

# the final answer as an assistant content line (no tool_calls).
Out-Bytes ((J ([pscustomobject]@{ role = 'assistant'; content = $reply })) + "`n")

# the end-of-run marker: the resume hint with the session id.
$hint = [pscustomobject]@{ role = 'meta'; type = 'session.resume_hint'; session_id = $id; command = "kimi -r $id"; content = "To resume this session: kimi -r $id" }
Out-Bytes ((J $hint) + "`n")

if ($env:FAKE_KIMI_EXIT) { exit ([int]$env:FAKE_KIMI_EXIT) }
exit 0
