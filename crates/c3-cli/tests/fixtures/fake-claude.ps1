# FAKE claude (Claude Code headless) for harness-claude.ps1 - never calls any model and never starts
# the real CLI. Started through fake-claude.cmd, which hands the command line over as
# FAKE_CLAUDE_ARGS (parsed here by the C runtime rules the bridge quotes by); the prompt arrives on
# STDIN as plain UTF-8. The bridge starts every claude child with an ALLOW-listed environment: the
# FAKE_CLAUDE_* drivers reach this fake only through the test hook CODEX_CONSULT_TEST_CHILD_ENV_PASS
# (test mode). Prints the events of `claude -p --output-format stream-json --verbose` shaped like
# Claude Code 2.1.285's: system/init {session_id, model, tools, mcp_servers, permissionMode,
# apiKeySource, cwd}, assistant {message{model, content[thinking|text|tool_use]}}, user
# {message{content[tool_result]}}, rate_limit_event {rate_limit_info}, system/compact_boundary, and
# the ONE result {subtype, is_error, num_turns, result, session_id, total_cost_usd, usage,
# modelUsage, permission_denials, structured_output?}.
#   FAKE_CLAUDE_ARGV_LOG=<path>   EVERY start appends one JSON line: {kind (turn | auth | version |
#                                 other), argv (the parsed arguments), stdin (the prompt), stdin_len,
#                                 cwd, env (the variable NAMES, sorted), autoupdater (the value of
#                                 DISABLE_AUTOUPDATER), schema_ok (the --json-schema text parses)} -
#                                 the harness's GUARD counts them (a child without a line fails)
#   `auth status`                 FAKE_CLAUDE_AUTH_STATUS=ok (default: loggedIn, claude.ai,
#                                 firstParty) | out (loggedIn false, exit 1) | apikey (authMethod
#                                 api_key) | console (authMethod console) | gateway (apiProvider
#                                 bedrock) | nojson (exit 0, text) | hang (40 s) | projdir=<path>
#                                 (projectsDirectory); the JSON carries a fake e-mail and org id
#                                 that must never reach a ledger
#   `--version`                   "2.1.285-fake (Claude Code)" (a real claude.exe would give 2.1.285.0)
#   print mode (-p):
#   FAKE_CLAUDE_REPLY=<file>      the reply: a JSON object -> result.structured_output (with
#                                 --json-schema) and its text as result.result; other text ->
#                                 result.result only
#   FAKE_CLAUDE_RESUME_REPLY=<file>  on a --resume turn (a secondary turn): this reply instead
#   FAKE_CLAUDE_PROSE=1           the reply text only, never structured_output (first turn)
#   FAKE_CLAUDE_IS_ERROR=<text>   result is_error true with that text; exit 1 unless EXIT is set
#   FAKE_CLAUDE_SUBTYPE=<s>       result.subtype (default success)
#   FAKE_CLAUDE_EXIT=<n>          exits n after the events
#   FAKE_CLAUDE_SESSION=other     the result names a new session (a resume or a new turn lands
#                                 elsewhere); =mismatch: init and result differ; =notuuid;
#                                 =parent: a fork answers on its parent; =notfound: --resume of an
#                                 unknown session ("No conversation found", exit 1, no events)
#   FAKE_CLAUDE_INIT_TOOLS=<csv>  the init's tools (default Glob,Grep,Read,StructuredOutput)
#   FAKE_CLAUDE_INIT_MCP=<name>   one MCP server in the init
#   FAKE_CLAUDE_INIT_MODE=<m>     the init's permissionMode (default: the --permission-mode)
#   FAKE_CLAUDE_INIT_MODEL=<m>    the init's model (default: an alias resolved - opus
#                                 claude-opus-5-5, sonnet claude-sonnet-5-5, haiku claude-haiku-4-5,
#                                 fable claude-fable-5-1 -, a full id as given without [1m])
#   FAKE_CLAUDE_APIKEYSOURCE=<v>  the init's apiKeySource (default: ANTHROPIC_API_KEY when that
#                                 variable reached the child, else none)
#   (wave 29b, E12-E15)
#   FAKE_CLAUDE_INIT_DROP=<csv>   these fields are left out of the init (tools, mcp_servers,
#                                 apiKeySource, model, permissionMode - a CLI whose schema changed)
#   FAKE_CLAUDE_ASSISTANT_MODEL=<m>  the message.model of every assistant event (default: the
#                                 init's model) - another model authoring the answer
#   FAKE_CLAUDE_INIT_SCOPE=new    INIT_TOOLS, INIT_MCP, INIT_DROP and ASSISTANT_MODEL apply only to
#                                 a turn without --resume (a continuation then inits cleanly)
#   FAKE_CLAUDE_RATE_RESET=<unix seconds>  the resetsAt of FAKE_CLAUDE_RATE_LIMIT's event (default:
#                                 in an hour)
#   FAKE_CLAUDE_MODEL_USAGE=<m>=<out>[,<m>=<out>...]  result.modelUsage (default: the init model)
#   FAKE_CLAUDE_DENIALS=1         a denied Read outside the working directories beside the reply
#                                 (a turn without --resume); =empty: ... and no reply; =all: every
#                                 turn, with a reply
#   FAKE_CLAUDE_RATE_LIMIT=rejected  a rejecting rate_limit_event (five_hour, resets in an hour)
#                                 and an is_error "usage limit" result, exit 1; =warning: an
#                                 allowed_warning event beside a normal answer; =rejected-success
#                                 (wave 29b, E15): the rejecting event, then the normal successful
#                                 answer (the CLI retried and got through)
#   FAKE_CLAUDE_HANG=1            sleeps 60 s after init; =new: only on a turn without --resume;
#                                 FAKE_CLAUDE_HANG_ON=<text>: only when the raw arguments hold it
#   FAKE_CLAUDE_TEXT=1            before the hang point: a thinking block, a text block and a Read
#                                 tool call with its result, tagged "(first ...)" / "(resume ...)"
#   FAKE_CLAUDE_TOOL_OPEN=1       before the hang point: a tool_use WITHOUT its tool_result
#   FAKE_CLAUDE_COMPACT=1         a system/compact_boundary event
#   FAKE_CLAUDE_WRITE=<rel path>  writes that file (relative to the working directory) on a turn
#                                 without --resume (=<path>|all: every turn)
#   FAKE_CLAUDE_NORESULT=1, FAKE_CLAUDE_TWORESULTS=1, FAKE_CLAUDE_BADLINE=1 (a non-JSON line
#                                 before the result), FAKE_CLAUDE_TRAILING=<text> (after the result,
#                                 no newline), FAKE_CLAUDE_NOINIT=1 (no init event)
#   FAKE_CLAUDE_STDERR=<text>     written to stderr
#   FAKE_CLAUDE_PIDFILE=<path>    its pid;  FAKE_CLAUDE_DELAY_MS=<ms>  sleeps before the answer
#   (wave 29b) the ENDPOINT mode - ANTHROPIC_BASE_URL reached the child (auth endpoint): the init's
#   model is the --model argument itself (no alias resolution; [1m] stripped), apiKeySource none
#   (as P8/P10 showed); FAKE_CLAUDE_TOKEN_EXPECT=<token>: when ANTHROPIC_AUTH_TOKEN differs, the
#   turn is P11's wrong token - init, then a result {subtype success, is_error true, "Failed to
#   authenticate. API Error: 401 ..."} without modelUsage, exit 1. The argv log line gains
#   base_url (the value of ANTHROPIC_BASE_URL), api_timeout_ms, has_token (ANTHROPIC_AUTH_TOKEN
#   set) and token_match (null without FAKE_CLAUDE_TOKEN_EXPECT) - never the token itself.
#   FAKE_CLAUDE_STDERR_NOTICE=1   writes `[claude-code:unrecognized_model] {"model":"<m>"}` to
#                                 stderr (P8: a notice, not a failure)
$ErrorActionPreference = 'Stop'
$u8 = New-Object System.Text.UTF8Encoding($false)
$raw = [string]$env:FAKE_CLAUDE_ARGS
function Out-Bytes { param([string]$Text) $b = $u8.GetBytes($Text); $s = [Console]::OpenStandardOutput(); $s.Write($b, 0, $b.Length); $s.Flush() }
function Err-Bytes { param([string]$Text) $b = $u8.GetBytes($Text + "`r`n"); $s = [Console]::OpenStandardError(); $s.Write($b, 0, $b.Length); $s.Flush() }
function J { param($Obj) return (ConvertTo-Json -InputObject $Obj -Compress -Depth 30) }
# (retried on a sharing violation: the members of a parallel panel share the harness's variables)
function Invoke-FakeWrite { param([scriptblock]$Do) for ($i = 0; $i -lt 60; $i++) { try { & $Do; return } catch { Start-Sleep -Milliseconds 50 } }; & $Do }

# the C runtime's argv rules (backslashes, \", quotes) - what claude.exe reads
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
if ($argv.Count -ge 2 -and $argv[0] -eq 'auth' -and $argv[1] -eq 'status') { $kind = 'auth' }
elseif ($argv -ccontains '--version') { $kind = 'version' }
elseif ($argv -ccontains '-p') { $kind = 'turn' }

$stdinText = ''
if ($kind -eq 'turn') {
    $stdinStream = [Console]::OpenStandardInput()
    $ms = New-Object System.IO.MemoryStream
    $stdinStream.CopyTo($ms)
    $stdinText = $u8.GetString($ms.ToArray())
}
$schemaText = Get-Flag '--json-schema'
$schemaOk = $null
if ($schemaText) { try { $so = ConvertFrom-Json -InputObject $schemaText; $schemaOk = [bool]($so.title -eq 'codex-consult reply v1') } catch { $schemaOk = $false } }
if ($env:FAKE_CLAUDE_ARGV_LOG) {
    $names = @([Environment]::GetEnvironmentVariables().Keys | ForEach-Object { [string]$_ } | Where-Object { $_ -ne 'FAKE_CLAUDE_ARGS' })
    $names = [string[]]$names
    [Array]::Sort($names, [StringComparer]::OrdinalIgnoreCase)
    $tokenMatch = $null
    if ($env:FAKE_CLAUDE_TOKEN_EXPECT) { $tokenMatch = [bool]([string]$env:ANTHROPIC_AUTH_TOKEN -ceq [string]$env:FAKE_CLAUDE_TOKEN_EXPECT) }
    $rec = [pscustomobject]@{ kind = $kind; argv = [object[]]$argv; stdin = $stdinText; stdin_len = $stdinText.Length; cwd = (Get-Location).Path; env = [object[]]$names; autoupdater = [string]$env:DISABLE_AUTOUPDATER; schema_ok = $schemaOk; pid = $PID; base_url = [string]$env:ANTHROPIC_BASE_URL; api_timeout_ms = [string]$env:API_TIMEOUT_MS; has_token = [bool]([string]$env:ANTHROPIC_AUTH_TOKEN); token_match = $tokenMatch }
    $line = (J $rec) + "`n"
    Invoke-FakeWrite { [IO.File]::AppendAllText($env:FAKE_CLAUDE_ARGV_LOG, $line, $u8) }
}
if ($env:FAKE_CLAUDE_PIDFILE -and $kind -eq 'turn') { Invoke-FakeWrite { [IO.File]::WriteAllText($env:FAKE_CLAUDE_PIDFILE, "$PID") } }

if ($kind -eq 'version') { Out-Bytes "2.1.285-fake (Claude Code)`n"; exit 0 }
if ($kind -eq 'auth') {
    $mode = if ($env:FAKE_CLAUDE_AUTH_STATUS) { [string]$env:FAKE_CLAUDE_AUTH_STATUS } else { 'ok' }
    if ($mode -eq 'hang') { Start-Sleep -Seconds 40; exit 0 }
    if ($mode -eq 'nojson') { Out-Bytes "You are logged in.`n"; exit 0 }
    $home2 = if ($env:USERPROFILE) { $env:USERPROFILE } else { [string]$env:HOME }
    $o = [ordered]@{ loggedIn = $true; authMethod = 'claude.ai'; apiProvider = 'firstParty'; analyticsDisabled = $false; projectsDirectory = (Join-Path $home2 '.claude\projects'); configDirectory = (Join-Path $home2 '.claude'); email = 'fake-person@example.invalid'; orgId = '00000000-0000-4000-8000-00000000fa4e'; orgName = 'Fake Org FAKE-ORG-NAME-7c2'; subscriptionType = 'max' }
    if ($mode -eq 'out') { $o = [ordered]@{ loggedIn = $false; authMethod = 'none'; apiProvider = 'firstParty' }; Out-Bytes ((ConvertTo-Json -InputObject ([pscustomobject]$o)) + "`n"); exit 1 }
    if ($mode -eq 'apikey') { $o['authMethod'] = 'api_key' }
    if ($mode -eq 'console') { $o['authMethod'] = 'console' }
    if ($mode -eq 'gateway') { $o['apiProvider'] = 'bedrock' }
    if ($mode -like 'projdir=*') { $o['projectsDirectory'] = $mode.Substring(8) }
    Out-Bytes ((ConvertTo-Json -InputObject ([pscustomobject]$o)) + "`n")
    exit 0
}
if ($kind -ne 'turn') { Err-Bytes "fake claude: unknown invocation: $raw"; exit 2 }

# ---- print mode
$resume = Get-Flag '--resume'
$fork = Test-Flag '--fork-session'
$sessionArg = Get-Flag '--session-id'
$modelArg = Get-Flag '--model'
$smode = [string]$env:FAKE_CLAUDE_SESSION
if ($resume -and $smode -eq 'notfound') { Err-Bytes "No conversation found with session ID: $resume"; exit 1 }
$id = if ($resume -and -not $fork) { $resume } elseif ($sessionArg) { $sessionArg } else { [guid]::NewGuid().ToString() }
if ($fork -and $smode -eq 'parent') { $id = $resume }
if ($smode -eq 'other') { $id = [guid]::NewGuid().ToString() }
if ($smode -eq 'notuuid') { $id = 'session-1' }
$resultId = $id
if ($smode -eq 'mismatch') { $resultId = [guid]::NewGuid().ToString() }
$base = ($modelArg -replace '\[1m\]$', '')
$aliases = @{ opus = 'claude-opus-5-5'; sonnet = 'claude-sonnet-5-5'; haiku = 'claude-haiku-4-5'; fable = 'claude-fable-5-1' }
# (wave 29b) the endpoint mode: a base URL reached the child - the id goes straight
$endpointMode = [bool]([string]$env:ANTHROPIC_BASE_URL)
$initModel = if ($env:FAKE_CLAUDE_INIT_MODEL) { [string]$env:FAKE_CLAUDE_INIT_MODEL } elseif ($endpointMode) { $base } elseif ($aliases.ContainsKey($base)) { $aliases[$base] } else { $base }
$wrongToken = ($endpointMode -and [bool]$env:FAKE_CLAUDE_TOKEN_EXPECT -and ([string]$env:ANTHROPIC_AUTH_TOKEN -cne [string]$env:FAKE_CLAUDE_TOKEN_EXPECT))
# (wave 29b) FAKE_CLAUDE_INIT_SCOPE=new: the init and assistant overrides on the first turn only
$scoped = ($env:FAKE_CLAUDE_INIT_SCOPE -ne 'new') -or (-not $resume)
$tools = if ($scoped -and $env:FAKE_CLAUDE_INIT_TOOLS) { @($env:FAKE_CLAUDE_INIT_TOOLS.Split(',')) } else { @('Glob', 'Grep', 'Read', 'StructuredOutput') }
# (wrapped in @(): an `if` that yields @() yields nothing - the init must carry [] as the real CLI's does, never null)
$mcp = @(if ($scoped -and $env:FAKE_CLAUDE_INIT_MCP) { [pscustomobject]@{ name = $env:FAKE_CLAUDE_INIT_MCP; status = 'connected' } })
$initDrop = if ($scoped -and $env:FAKE_CLAUDE_INIT_DROP) { @($env:FAKE_CLAUDE_INIT_DROP.Split(',') | ForEach-Object { $_.Trim() }) } else { @() }
$asstModel = if ($scoped -and $env:FAKE_CLAUDE_ASSISTANT_MODEL) { [string]$env:FAKE_CLAUDE_ASSISTANT_MODEL } else { $initModel }
$pmode = if ($env:FAKE_CLAUDE_INIT_MODE) { [string]$env:FAKE_CLAUDE_INIT_MODE } else { Get-Flag '--permission-mode' }
$keySource = if ($env:FAKE_CLAUDE_APIKEYSOURCE) { [string]$env:FAKE_CLAUDE_APIKEYSOURCE } elseif ($env:ANTHROPIC_API_KEY) { 'ANTHROPIC_API_KEY' } else { 'none' }
$tag = if ($resume) { 'resume' } else { 'first' }
if ($env:FAKE_CLAUDE_NOINIT -ne '1') {
    $initEv = [ordered]@{ type = 'system'; subtype = 'init'; cwd = (Get-Location).Path; session_id = $id; tools = [object[]]$tools; mcp_servers = [object[]]$mcp; model = $initModel; permissionMode = $pmode; slash_commands = @(); apiKeySource = $keySource; output_style = 'default'; agents = @('general-purpose'); skills = @(); plugins = @([pscustomobject]@{ name = 'cc-plugin-agents-md' }); uuid = [guid]::NewGuid().ToString() }
    foreach ($dk in $initDrop) { if ($initEv.Contains($dk)) { $initEv.Remove($dk) } }
    Out-Bytes ((J ([pscustomobject]$initEv)) + "`n")
}
if ($env:FAKE_CLAUDE_TEXT) {
    Out-Bytes ((J ([pscustomobject]@{ type = 'assistant'; message = [pscustomobject]@{ model = $asstModel; content = @([pscustomobject]@{ type = 'thinking'; thinking = "Planning the review ($tag turn)." }, [pscustomobject]@{ type = 'text'; text = "Reading the brief ($tag turn)." }, [pscustomobject]@{ type = 'tool_use'; id = "toolu_$tag"; name = 'Read'; input = [pscustomobject]@{ file_path = "app.txt" } }) }; session_id = $id })) + "`n")
    Out-Bytes ((J ([pscustomobject]@{ type = 'user'; message = [pscustomobject]@{ content = @([pscustomobject]@{ type = 'tool_result'; tool_use_id = "toolu_$tag"; content = 'one' }) }; session_id = $id })) + "`n")
}
if ($env:FAKE_CLAUDE_TOOL_OPEN -eq '1') {
    Out-Bytes ((J ([pscustomobject]@{ type = 'assistant'; message = [pscustomobject]@{ model = $asstModel; content = @([pscustomobject]@{ type = 'tool_use'; id = 'toolu_open'; name = 'Grep'; input = [pscustomobject]@{ pattern = 'x' } }) }; session_id = $id })) + "`n")
}
if ($env:FAKE_CLAUDE_HANG -eq '1' -or ($env:FAKE_CLAUDE_HANG -eq 'new' -and -not $resume) -or ($env:FAKE_CLAUDE_HANG_ON -and $raw.Contains($env:FAKE_CLAUDE_HANG_ON))) { Start-Sleep -Seconds 60 }
if ($env:FAKE_CLAUDE_DELAY_MS) { Start-Sleep -Milliseconds ([int]$env:FAKE_CLAUDE_DELAY_MS) }
if ($env:FAKE_CLAUDE_COMPACT -eq '1') { Out-Bytes ((J ([pscustomobject]@{ type = 'system'; subtype = 'compact_boundary'; session_id = $id })) + "`n") }
if ($env:FAKE_CLAUDE_WRITE) {
    $w = $env:FAKE_CLAUDE_WRITE.Split('|')
    if (-not $resume -or ($w.Count -gt 1 -and $w[1] -eq 'all')) {
        $target = Join-Path (Get-Location).Path $w[0]
        [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target))
        [IO.File]::WriteAllText($target, "written by the fake claude`n", $u8)
    }
}
if ($env:FAKE_CLAUDE_STDERR) { Err-Bytes $env:FAKE_CLAUDE_STDERR }
if ($env:FAKE_CLAUDE_STDERR_NOTICE -eq '1') { Err-Bytes ('[claude-code:unrecognized_model] {"model":"' + $base + '"}') }
# (wave 29b, P11) a wrong token against the endpoint: no fallback to the local login
if ($wrongToken) {
    Out-Bytes ((J ([pscustomobject]@{ type = 'result'; subtype = 'success'; is_error = $true; duration_ms = 187000; duration_api_ms = 0; num_turns = 1; result = 'Failed to authenticate. API Error: 401 {"type":"error","error":{"type":"authentication_error","message":"token expired or incorrect"}}'; session_id = $id; total_cost_usd = 0; usage = [pscustomobject]@{ input_tokens = 0; cache_creation_input_tokens = 0; cache_read_input_tokens = 0; output_tokens = 0 }; permission_denials = [object[]]@(); uuid = [guid]::NewGuid().ToString() })) + "`n")
    exit 1
}
if ($env:FAKE_CLAUDE_BADLINE -eq '1') { Out-Bytes "this is not json`n" }
$rl = [string]$env:FAKE_CLAUDE_RATE_LIMIT
if ($rl) {
    $resets = if ($env:FAKE_CLAUDE_RATE_RESET) { [long]$env:FAKE_CLAUDE_RATE_RESET } else { [DateTimeOffset]::UtcNow.AddHours(1).ToUnixTimeSeconds() }
    $status = if ($rl -like 'rejected*') { 'rejected' } else { 'allowed_warning' }
    Out-Bytes ((J ([pscustomobject]@{ type = 'rate_limit_event'; rate_limit_info = [pscustomobject]@{ status = $status; resetsAt = $resets; rateLimitType = 'five_hour'; utilization = 0.97 }; uuid = [guid]::NewGuid().ToString(); session_id = $id })) + "`n")
}

# the reply
$replyFile = $env:FAKE_CLAUDE_REPLY
$fromResume = $false
if ($resume -and $env:FAKE_CLAUDE_RESUME_REPLY) { $replyFile = $env:FAKE_CLAUDE_RESUME_REPLY; $fromResume = $true }
$den = [string]$env:FAKE_CLAUDE_DENIALS
$denied = ($den -eq 'all') -or (($den -eq '1' -or $den -eq 'empty') -and -not $resume)
$emptyByDenial = ($den -eq 'empty' -and -not $resume)
$resultText = ''
$structured = $null
if ($replyFile -and -not $emptyByDenial) {
    $text = [IO.File]::ReadAllText($replyFile, $u8)
    $resultText = $text
    $obj = $null
    if ($schemaText -and -not ($env:FAKE_CLAUDE_PROSE -eq '1' -and -not $fromResume)) { try { $obj = ConvertFrom-Json -InputObject $text } catch { $obj = $null } }
    if ($null -ne $obj -and $obj -is [System.Management.Automation.PSCustomObject]) { $structured = $obj; $resultText = J $obj }
}
$isError = $false
$exitCode = 0
if ($rl -eq 'rejected') { $isError = $true; $resultText = "Claude AI usage limit reached|$resets"; $structured = $null; $exitCode = 1 }
if ($env:FAKE_CLAUDE_IS_ERROR) { $isError = $true; $resultText = [string]$env:FAKE_CLAUDE_IS_ERROR; $structured = $null; $exitCode = 1 }
$subtype = if ($env:FAKE_CLAUDE_SUBTYPE) { [string]$env:FAKE_CLAUDE_SUBTYPE } else { 'success' }
if ($denied) {
    Out-Bytes ((J ([pscustomobject]@{ type = 'assistant'; message = [pscustomobject]@{ model = $asstModel; content = @([pscustomobject]@{ type = 'tool_use'; id = 'toolu_denied'; name = 'Read'; input = [pscustomobject]@{ file_path = 'C:\outside\secret.txt' } }) }; session_id = $id })) + "`n")
    Out-Bytes ((J ([pscustomobject]@{ type = 'user'; message = [pscustomobject]@{ content = @([pscustomobject]@{ type = 'tool_result'; tool_use_id = 'toolu_denied'; is_error = $true; content = 'C:\outside\secret.txt is outside the working directories' }) }; session_id = $id })) + "`n")
}
if ($resultText -and -not $isError) {
    Out-Bytes ((J ([pscustomobject]@{ type = 'assistant'; message = [pscustomobject]@{ model = $asstModel; content = @([pscustomobject]@{ type = 'text'; text = "Answer ($tag turn)." }) }; session_id = $id })) + "`n")
}
if ($env:FAKE_CLAUDE_NORESULT -ne '1') {
    $mu = [ordered]@{}
    if ($env:FAKE_CLAUDE_MODEL_USAGE) { foreach ($pair in $env:FAKE_CLAUDE_MODEL_USAGE.Split(',')) { $kv = $pair.Split('='); $mu[$kv[0]] = [pscustomobject]@{ inputTokens = 100; outputTokens = [int]$kv[1]; cacheReadInputTokens = 0; cacheCreationInputTokens = 0; costUSD = 0.01 } } }
    else { $mu[$initModel] = [pscustomobject]@{ inputTokens = 12; outputTokens = 480; cacheReadInputTokens = 9000; cacheCreationInputTokens = 3000; costUSD = 0.05 } }
    $usage = if ($resume) { [pscustomobject]@{ input_tokens = 8; cache_creation_input_tokens = 500; cache_read_input_tokens = 30000; output_tokens = 300 } } else { [pscustomobject]@{ input_tokens = 12; cache_creation_input_tokens = 3000; cache_read_input_tokens = 9000; output_tokens = 480 } }
    $res = [ordered]@{ type = 'result'; subtype = $subtype; is_error = $isError; duration_ms = 3100; duration_api_ms = 2900; num_turns = 2; result = $resultText; session_id = $resultId; total_cost_usd = 0.0512; usage = $usage; modelUsage = [pscustomobject]$mu }
    # (assigned directly: $(...) would unroll an empty array, and Windows PowerShell 5.1 then writes
    # {} where the real CLI writes [])
    if ($denied) { $res['permission_denials'] = [object[]]@([pscustomobject]@{ tool_name = 'Read'; tool_use_id = 'toolu_denied'; tool_input = [pscustomobject]@{ file_path = 'C:\outside\secret.txt' } }) }
    else { $res['permission_denials'] = [object[]]@() }
    if ($null -ne $structured) { $res['structured_output'] = $structured }
    $res['stop_reason'] = 'end_turn'
    $res['uuid'] = [guid]::NewGuid().ToString()
    $line = (J ([pscustomobject]$res)) + "`n"
    Out-Bytes $line
    if ($env:FAKE_CLAUDE_TWORESULTS -eq '1') { Out-Bytes $line }
}
if ($env:FAKE_CLAUDE_TRAILING) { Out-Bytes $env:FAKE_CLAUDE_TRAILING }
if ($env:FAKE_CLAUDE_EXIT) { exit ([int]$env:FAKE_CLAUDE_EXIT) }
exit $exitCode
