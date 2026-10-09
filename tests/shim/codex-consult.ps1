<#
.SYNOPSIS
    Prototype shim: fronts the C3 `c3 consult` binary with the parameter surface that
    `codex-consult.ps1` exposes to the codex-consult plugin's PowerShell harnesses.

.DESCRIPTION
    See C:\Users\Dmytro\C3\docs\port\cli-surface.md (section 1) for the full parameter
    mapping this shim implements, and C:\Users\Dmytro\C3\docs\port\m2-acceptance.md for
    the acceptance checklist it is meant to satisfy. See tests\shim\codex-providers.ps1
    for the pattern this file follows (binary resolution, thin forwarding, exit-code
    passthrough) - this file repeats that pattern verbatim, extended for `c3 consult`'s
    much larger parameter surface.

    A consultation is started the same way from any coordinator: neither the shim nor c3
    assumes a host for the run itself.

    THIN FORWARDER ONLY: no TOML scanning, no preflight, no ledger/lock logic here. It
    locates the `c3` binary, maps this script's named parameters onto `c3 consult`
    flags, runs it with stdin/stdout/stderr connected straight through, and exits with
    the binary's own exit code.

    `-Artifact` accepts the plugin's comma-joined single-string form (a `-File` invocation
    can only bind a `[string[]]` parameter once) and is split on comma here into repeatable
    `--artifact` flags. (wave 2b) `-CodexConfig` is passed through WHOLE, one `--codex-config`
    per bound string: c3 splits a comma-separated value itself, keeping a comma inside
    brackets or quotes (`model_x=[1,2]`), exactly as the plugin does - the shim's own comma
    split broke such a value (harness-0.3 CFG).

    (wave 2b) `-Task` is optional, as in the plugin (0.6.x): `-Explain coordinate|consult|
    providers` is the one form without it, and every other form reaches c3 without `--task`,
    which refuses it with the plugin's text (exit 1) - a Mandatory parameter made PowerShell
    PROMPT on the harness's open stdin and hung harness-host. `-Explain` is forwarded with
    every other bound parameter (c3 refuses "-Explain takes no other parameter").

    (wave 2b) The plugin's own directory: this shim sets CLAUDE_PLUGIN_ROOT to the parent of
    its directory (the staged plugin tree - its templates/ for the role files, its skills/ for
    -Explain), as the plugin resolves `Split-Path -Parent $PSScriptRoot`; and C3_SCHEMA_FILE to
    `<scripts>\..\schemas\consult-reply.schema.json` when that file exists (c3 passes it to
    the engine - `--output-schema` / `--json-schema` - when it holds the embedded schema).

    `-FormatRetry`/`-DenialRetry` are ported from the plugin's `int` (0/1) parameters to
    boolean `--format-retry`/`--no-format-retry` and `--denial-retry`/`--no-denial-retry`
    flag pairs (cli-surface.md Open question 4); this shim accepts the plugin's 0/1
    ints and maps them, so a caller written against the plugin's own contract needs no
    changes.

    `c3` binary resolution order (identical to tests\shim\codex-providers.ps1):
      1. $env:C3_EXE, if set.
      2. <repo root>\target\debug\c3.exe (two levels up from this file).
      3. `c3` (or `c3.exe`) resolved via PATH.
    If none of these exist, the shim exits 127 with a message on stderr.

.NOTES
    NOT YET RUNNABLE END TO END: `c3 consult` is being implemented in parallel by
    another worker and does not exist at the time this shim was written. Validate by
    parsing only:

      powershell -NoProfile -Command "$errs=$null; [void][System.Management.Automation.Language.Parser]::ParseFile('C:\Users\Dmytro\C3\tests\shim\codex-consult.ps1', [ref]$null, [ref]$errs); if ($errs) { $errs | ForEach-Object { Write-Host $_ } ; exit 1 } else { Write-Host 'parse OK' }"
#>
[CmdletBinding()]
param(
    # (wave 2b) not Mandatory: -Explain is the one form without it; c3 refuses the others.
    [string]$Task = '',

    # Position 0 so a bare positional (e.g. `-Status abcd1234`) binds here, as the plugin's own
    # param block does - c3 then refuses it with the -Id hint.
    [Parameter(Position = 0)]
    [string]$CollabDir = '.collab',
    [string]$Mode = '',
    [string]$Thread = '',
    [string]$Brief = '',
    [string]$Prompt = '',
    [string]$Model = '',
    [string]$Purpose = '',
    [string]$Effort = '',
    [string]$Sandbox = 'read-only',
    [int]$MaxWords = 0,
    [int]$TimeoutSec = 0,
    [int]$ContinueSec = -1,
    [int]$StallSec = -1,
    [string]$Range = '',
    [string]$ReplyName = 'reply',
    [string[]]$Artifact = @(),
    [switch]$Raw,
    [string]$CodexExe = '',
    [string]$Provider = '',
    [string]$NativeEffort = '',
    [switch]$OffPeakOnly,
    [switch]$SkipPreflight,
    [string[]]$CodexConfig = @(),
    [string]$SchemaTransport = '',
    [int]$FormatRetry = 1,
    [switch]$Panel,
    [switch]$PanelAll,
    # INTERNAL: set by -Panel for each member run. Never pass it yourself.
    [string]$PanelSpec = '',
    [int]$PanelConcurrency = 0,
    # -1 = not given (c3's clap default); >=1 forwarded (0/negatives reach the validator).
    [int]$PanelSize = -1,
    [string]$PanelOrder = '',
    [string]$PanelSeed = '',
    [string[]]$Require = @(),
    [string]$Role = '',
    [string[]]$Roles = @(),
    [string[]]$Topic = @(),
    [string]$Engine = '',
    [string]$EngineExe = '',
    [int]$MaxModelSteps = 0,
    [int]$DenialRetry = 1,
    [switch]$DryRun,
    # (wave 2c) on | off for this run (it wins over CODEX_CONSULT_TELEMETRY); forwarded as
    # --telemetry. C3 posts only to CODEX_CONSULT_TELEMETRY_URL (a harness's loopback intake).
    [string]$Telemetry = '',

    # RESERVED (R12 design; not yet in codex-consult.ps1's own param block - see
    # cli-surface.md section 1 and Open question 2). Forwarded when passed so the shim
    # does not need to change again once R12 lands.
    [switch]$Detach,
    [string]$DetachId = '',
    [switch]$Status,
    [string]$Id = '',
    [switch]$List,
    [switch]$Wait,
    [int]$WaitTimeoutSec = 0,
    [switch]$Prune,

    # (wave 26b, D10) -Kick -Member <NN> [-Id <id8>]: stop one running member of -Task.
    [switch]$Kick,
    [string]$Member = '',

    # (wave 27, R13 D5) coordinate | consult | providers: a skill's text for a host without skills.
    [string]$Explain = ''
)

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

$c3 = Resolve-C3Exe
if (-not $c3) {
    Write-Error "codex-consult shim: could not locate the c3 binary (checked `$env:C3_EXE, target\debug\c3.exe relative to the repo root, and PATH)."
    exit 127
}

# The plugin's own codex-consult.ps1 IS the bridge process: the harness monitors and kills THIS
# process (the one it launched via Start-Process) and reads its pid from the lock and recovery
# records. c3 runs as a child of this shim, so it would otherwise record its own (different) pid and
# outlive this shim when killed. Export this shim's pid as the bridge so c3 records it and shares
# this shim's fate (a test-only hook; unset in production, where c3 is launched directly).
# (wave 27c, D14) a CODEX_CONSULT_TEST_* hook is honoured by c3 only when CODEX_CONSULT_TEST_MODE
# is set too. The plugin's wave-27c harnesses each set the mode themselves at their top, so this
# shim must NOT force it: a run that deliberately UNSETS the mode (D14, to verify a hook is then
# ignored) must reach c3 with the mode off. The caller's value is passed through untouched.
$env:CODEX_CONSULT_TEST_BRIDGE_PID = "$PID"

# (wave 2b) the plugin's root is the directory above its scripts (the staged plugin tree): its
# templates/ (role files) and skills/ (-Explain). The plugin derives it from $PSScriptRoot, never
# from the environment, so the shim sets it whatever the caller had.
$env:CLAUDE_PLUGIN_ROOT = Split-Path -Parent $PSScriptRoot
# (wave 2b) the plugin's schema file beside its scripts: c3 names it in the engine's argv
# (--output-schema / --json-schema) when it holds exactly the embedded schema.
$schemaFile = [IO.Path]::GetFullPath((Join-Path (Split-Path -Parent $PSScriptRoot) 'schemas\consult-reply.schema.json'))
if (-not $env:C3_SCHEMA_FILE -and (Test-Path -LiteralPath $schemaFile -PathType Leaf)) { $env:C3_SCHEMA_FILE = $schemaFile }

# (wave 27, R13 D5) -Explain: forwarded with every other bound parameter (c3 refuses those).
if ($PSBoundParameters.ContainsKey('Explain')) {
    $x = New-Object System.Collections.Generic.List[string]
    $x.Add('consult'); $x.Add('--explain'); $x.Add($Explain)
    foreach ($kv in $PSBoundParameters.GetEnumerator()) {
        if ($kv.Key -eq 'Explain') { continue }
        $flag = '--' + (($kv.Key -creplace '([a-z0-9])([A-Z])', '$1-$2').ToLowerInvariant())
        if ($kv.Value -is [System.Management.Automation.SwitchParameter]) { $x.Add($flag) }
        else { $x.Add($flag); $x.Add((@($kv.Value) -join ',')) }
    }
    $x = Get-NativeArgs $x
    & $c3 @x
    exit $LASTEXITCODE
}

# The harnesses run fake consultations: nothing of them may reach the telemetry hub and no
# priors download may start. A value set by the caller wins.
if (-not $env:CODEX_CONSULT_TELEMETRY) { $env:CODEX_CONSULT_TELEMETRY = "off" }
# (wave 3b) C3 keeps its telemetry under <codex home>\c3\telemetry\ (P7); in TEST MODE its not-spooled
# count, its forgetting marker and its last flush's record go to the plugin's places under the codex
# home instead (C3_TEST_TELEMETRY_PLUGIN_HOME - c3 honours it in test mode only), so the plugin's
# harnesses run C3's count, fold and marker on the plugin's own files. A caller-set value wins.
if (-not $env:C3_TEST_TELEMETRY_PLUGIN_HOME -and $env:CODEX_HOME -and ([string]$env:CODEX_CONSULT_TEST_MODE).Trim() -match '^(?i:1|true|yes|on)$') { $env:C3_TEST_TELEMETRY_PLUGIN_HOME = $env:CODEX_HOME }

# (wave 26b, D10) -Kick / -Member: a control action, not a run. Forward only the flags c3's kick
# dispatch reads (task, collab-dir, kick, member, id) so the run-path defaults never reach it.
if ($Kick -or $PSBoundParameters.ContainsKey('Member')) {
    $k = New-Object System.Collections.Generic.List[string]
    $k.Add('consult'); if ($Task) { $k.Add('--task'); $k.Add($Task) }
    if ($PSBoundParameters.ContainsKey('CollabDir')) { $k.Add('--collab-dir'); $k.Add($CollabDir) }
    if ($Kick) { $k.Add('--kick') }
    if ($PSBoundParameters.ContainsKey('Member')) { $k.Add('--member'); $k.Add($Member) }
    if ($PSBoundParameters.ContainsKey('Id')) { $k.Add('--id'); $k.Add($Id) }
    $k = Get-NativeArgs $k
    & $c3 @k
    exit $LASTEXITCODE
}

# A detached QUERY (-Status / -Wait): the plugin forwards only -Task, -CollabDir, -Id, -Prune and
# -WaitTimeoutSec (plus any bound run option, so c3 can refuse it). Only bound parameters are
# forwarded so c3's "-Status and -Wait take only ..." check sees exactly what the caller passed.
if ($Status -or $Wait) {
    $q = New-Object System.Collections.Generic.List[string]
    $q.Add('consult'); if ($Task) { $q.Add('--task'); $q.Add($Task) }
    if ($PSBoundParameters.ContainsKey('CollabDir')) { $q.Add('--collab-dir'); $q.Add($CollabDir) }
    if ($Status) { $q.Add('--status') }
    if ($Wait) { $q.Add('--wait') }
    if ($PSBoundParameters.ContainsKey('Id')) { $q.Add('--id'); $q.Add($Id) }
    if ($PSBoundParameters.ContainsKey('WaitTimeoutSec')) { $q.Add('--wait-timeout-sec'); $q.Add([string]$WaitTimeoutSec) }
    if ($Prune) { $q.Add('--prune') }
    if ($Detach) { $q.Add('--detach') }
    # bound run options: forwarded so c3 refuses the query with the "not -X" wording
    if ($PSBoundParameters.ContainsKey('Mode')) { $q.Add('--mode'); $q.Add($Mode) }
    if ($PSBoundParameters.ContainsKey('Thread')) { $q.Add('--thread'); $q.Add($Thread) }
    if ($PSBoundParameters.ContainsKey('Brief')) { $q.Add('--brief'); $q.Add($Brief) }
    if ($PSBoundParameters.ContainsKey('Prompt')) { $q.Add('--prompt'); $q.Add($Prompt) }
    if ($PSBoundParameters.ContainsKey('Provider')) { $q.Add('--provider'); $q.Add($Provider) }
    if ($PSBoundParameters.ContainsKey('Model')) { $q.Add('--model'); $q.Add($Model) }
    if ($PSBoundParameters.ContainsKey('Purpose')) { $q.Add('--purpose'); $q.Add($Purpose) }
    if ($PSBoundParameters.ContainsKey('Engine')) { $q.Add('--engine'); $q.Add($Engine) }
    if ($Panel) { $q.Add('--panel') }
    if ($PanelAll) { $q.Add('--panel-all') }
    $q = Get-NativeArgs $q
    & $c3 @q
    exit $LASTEXITCODE
}

$c3Args = New-Object System.Collections.Generic.List[string]
$c3Args.Add('consult')

if ($Task) { $c3Args.Add('--task'); $c3Args.Add($Task) }
if ($CollabDir) { $c3Args.Add('--collab-dir'); $c3Args.Add($CollabDir) }
if ($Mode) { $c3Args.Add('--mode'); $c3Args.Add($Mode) }
if ($Thread) { $c3Args.Add('--thread'); $c3Args.Add($Thread) }
if ($Brief) { $c3Args.Add('--brief'); $c3Args.Add($Brief) }
if ($Prompt) { $c3Args.Add('--prompt'); $c3Args.Add($Prompt) }
if ($Model) { $c3Args.Add('--model'); $c3Args.Add($Model) }
if ($Purpose) { $c3Args.Add('--purpose'); $c3Args.Add($Purpose) }
if ($Effort) { $c3Args.Add('--effort'); $c3Args.Add($Effort) }
if ($Sandbox) { $c3Args.Add('--sandbox'); $c3Args.Add($Sandbox) }
if ($MaxWords -ne 0) { $c3Args.Add('--max-words'); $c3Args.Add([string]$MaxWords) }
if ($TimeoutSec -ne 0) { $c3Args.Add('--timeout-sec'); $c3Args.Add([string]$TimeoutSec) }
if ($ContinueSec -ne -1) { $c3Args.Add('--continue-sec'); $c3Args.Add([string]$ContinueSec) }
if ($StallSec -ne -1) { $c3Args.Add('--stall-sec'); $c3Args.Add([string]$StallSec) }
if ($Range) { $c3Args.Add('--range'); $c3Args.Add($Range) }
if ($ReplyName) { $c3Args.Add('--reply-name'); $c3Args.Add($ReplyName) }
foreach ($a in $Artifact) {
    foreach ($piece in ($a -split ',')) {
        if ($piece) { $c3Args.Add('--artifact'); $c3Args.Add($piece) }
    }
}
if ($Raw) { $c3Args.Add('--raw') }
if ($CodexExe) { $c3Args.Add('--codex-exe'); $c3Args.Add($CodexExe) }
if ($Provider) { $c3Args.Add('--provider'); $c3Args.Add($Provider) }
if ($NativeEffort) { $c3Args.Add('--native-effort'); $c3Args.Add($NativeEffort) }
if ($OffPeakOnly) { $c3Args.Add('--off-peak-only') }
if ($SkipPreflight) { $c3Args.Add('--skip-preflight') }
# (wave 2b) whole: c3 splits a comma-separated value itself (a comma inside [..] or quotes stays)
foreach ($c in $CodexConfig) {
    if ($c) { $c3Args.Add('--codex-config'); $c3Args.Add($c) }
}
if ($SchemaTransport) { $c3Args.Add('--schema-transport'); $c3Args.Add($SchemaTransport) }
# c3's --format-retry takes a value (0|1); it is NOT a boolean --format-retry/--no-format-retry
# pair (that was the cli-surface.md design proposal - the actual binary kept the plugin's own
# int semantics). Confirmed against `c3 consult --help`.
$c3Args.Add('--format-retry'); $c3Args.Add([string]$FormatRetry)
if ($Panel) { $c3Args.Add('--panel') }
if ($PanelAll) { $c3Args.Add('--panel-all') }
if ($PanelSpec) { $c3Args.Add('--panel-spec'); $c3Args.Add($PanelSpec) }
if ($PanelConcurrency -ne 0) { $c3Args.Add('--panel-concurrency'); $c3Args.Add([string]$PanelConcurrency) }
if ($PanelSize -ne -1) { $c3Args.Add('--panel-size'); $c3Args.Add([string]$PanelSize) }
if ($PanelOrder) { $c3Args.Add('--panel-order'); $c3Args.Add($PanelOrder) }
if ($PanelSeed) { $c3Args.Add('--panel-seed'); $c3Args.Add($PanelSeed) }
foreach ($rq in $Require) {
    foreach ($piece in ($rq -split ',')) {
        if ($piece) { $c3Args.Add('--require'); $c3Args.Add($piece) }
    }
}
if ($Role) { $c3Args.Add('--role'); $c3Args.Add($Role) }
foreach ($rl in $Roles) {
    foreach ($piece in ($rl -split ',')) {
        if ($piece) { $c3Args.Add('--roles'); $c3Args.Add($piece) }
    }
}
foreach ($tp in $Topic) {
    foreach ($piece in ($tp -split ',')) {
        if ($piece) { $c3Args.Add('--topic'); $c3Args.Add($piece) }
    }
}
if ($Engine) { $c3Args.Add('--engine'); $c3Args.Add($Engine) }
if ($EngineExe) { $c3Args.Add('--engine-exe'); $c3Args.Add($EngineExe) }
if ($MaxModelSteps -ne 0) { $c3Args.Add('--max-model-steps'); $c3Args.Add([string]$MaxModelSteps) }
# Same as --format-retry above: c3's --denial-retry takes a value (0|1), not a boolean pair.
$c3Args.Add('--denial-retry'); $c3Args.Add([string]$DenialRetry)
if ($DryRun) { $c3Args.Add('--dry-run') }
if ($Telemetry) { $c3Args.Add('--telemetry'); $c3Args.Add($Telemetry) }

# R12 flags (a run / the -Detach foreground / the -DetachId background; -Status/-Wait handled
# above). -Id/-Prune/-WaitTimeoutSec without -Status/-Wait are forwarded so c3 refuses them.
if ($Detach) { $c3Args.Add('--detach') }
if ($DetachId) { $c3Args.Add('--detach-id'); $c3Args.Add($DetachId) }
if ($PSBoundParameters.ContainsKey('Id')) { $c3Args.Add('--id'); $c3Args.Add($Id) }
if ($List) { $c3Args.Add('--list') }
if ($PSBoundParameters.ContainsKey('WaitTimeoutSec')) { $c3Args.Add('--wait-timeout-sec'); $c3Args.Add([string]$WaitTimeoutSec) }
if ($Prune) { $c3Args.Add('--prune') }

# Forward stdout/stderr unchanged so a harness's captured 2>&1 text is exactly what c3
# printed - no Write-Host wrapping, no re-encoding.
$c3Args = Get-NativeArgs $c3Args
& $c3 @c3Args
exit $LASTEXITCODE
