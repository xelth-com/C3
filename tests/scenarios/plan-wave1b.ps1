# Wave 1b (0.6.1 parity): the roster `plan` through the shims WITHOUT the claude engine - the
# harness-claude checks ENDPOINT E5 (listing, -Short, walk, direct-run refusal, both ways, no plan),
# E7 through the bridge, ACCEPT E15/E16 (machine-wide plan hold; the two-repository plan wait),
# replayed with two CODEX routes of one plan (ZAI and ZAIB, two endpoints) instead of a codex and a
# claude endpoint route. Run under the HARNESS.lock mutex, one at a time.
param([string]$Scripts, [string]$Fake, [string]$Work)
$ErrorActionPreference = 'Stop'
$u8 = New-Object Text.UTF8Encoding($false)
$psExe = (Get-Process -Id $PID).Path
if (Test-Path $Work) { Remove-Item $Work -Recurse -Force }
[void][IO.Directory]::CreateDirectory($Work)
$consultPs = Join-Path $Scripts 'codex-consult.ps1'
$providersPs = Join-Path $Scripts 'codex-providers.ps1'
$script:pass = 0; $script:fail = 0
function Check { param([string]$Id, [string]$What, [bool]$Ok, [string]$Ev = '')
    if ($Ok) { $script:pass++ } else { $script:fail++ }
    if ($Ev.Length -gt 400) { $Ev = $Ev.Substring(0, 400) + '...' }
    Write-Host ("{0} {1,-6} {2}{3}" -f $(if ($Ok) { 'ok  ' } else { 'FAIL' }), $Id, $What, $(if ($Ev) { "  | $Ev" } else { '' })) }
function G { param([string]$Repo, [string[]]$A) $p = $ErrorActionPreference; $ErrorActionPreference = 'Continue'; $o = & git -C $Repo @A 2>&1; $ErrorActionPreference = $p; return $o }
function New-Repo { param([string]$Name)
    $r = Join-Path $Work $Name
    [void][IO.Directory]::CreateDirectory($r)
    $null = G $r @('init', '-q'); $null = G $r @('config', 'user.email', 't@e.com'); $null = G $r @('config', 'user.name', 'T')
    [IO.File]::WriteAllText((Join-Path $r 'app.txt'), "one`n", $u8)
    $null = G $r @('add', '-A'); $null = G $r @('commit', '-q', '-m', 'init')
    [void][IO.Directory]::CreateDirectory((Join-Path $r '.collab\t\handoffs'))
    return $r }
$codexHome = Join-Path $Work 'home'
[void][IO.Directory]::CreateDirectory($codexHome)
$toml = "model = `"gpt-5.1`"`n`n[model_providers.ZAI]`nbase_url = `"https://api.z.ai/api/v1`"`nenv_key = `"RT_ZAI_KEY`"`nwire_api = `"responses`"`n`n[model_providers.ZAIB]`nbase_url = `"https://open.bigmodel.cn/api/paas/v4`"`nenv_key = `"RT_ZAIB_KEY`"`nwire_api = `"responses`"`n"
[IO.File]::WriteAllText((Join-Path $codexHome 'config.toml'), $toml, $u8)
function Write-File { param([string]$Name, [string]$Text) $p = Join-Path $Work $Name; [IO.File]::WriteAllText($p, $Text, $u8); return $p }
$advise = Write-File 'advise.json' '{"schema_version":"1","verdict":"ADVISE","verdict_reason":"r","reply_markdown":"m","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}'
$zai = '{"provider":"ZAI","model":"glm-5.3","plan":"zai"}'
$zaib = '{"provider":"ZAIB","model":"glm-5.3","plan":"zai"}'
$oai = '{"provider":"openai","model":"gpt-5.1"}'
$rosterPlan = Write-File 'roster-plan.json' ('{"roster_version":1,"reviewers":[' + (@($zai, $zaib, $oai) -join ',') + ']}')
$rosterNoPlan = Write-File 'roster-noplan.json' '{"roster_version":1,"reviewers":[{"provider":"ZAI","model":"glm-5.3"},{"provider":"ZAIB","model":"glm-5.3"},{"provider":"openai","model":"gpt-5.1"}]}'
$rosterPar = Write-File 'roster-par.json' ('{"roster_version":1,"reviewers":[' + (@($zai, $zaib, $oai) -join ',') + '],"parallel":{"zai":2}}')
$limitMsg = "You've hit your usage limit. Upgrade to Pro or try again in 3 days 1 hour 7 minutes."

function Set-Env { param([string]$Roster, [hashtable]$Env)
    foreach ($k in @(Get-ChildItem env: | Where-Object { $_.Name -like 'FAKE_CODEX_*' } | ForEach-Object { $_.Name })) { Remove-Item "env:$k" }
    $env:CODEX_HOME = $codexHome; $env:CODEX_CONSULT_TEST_MODE = '1'; $env:CODEX_CONSULT_HEALTH = 'none'
    $env:RT_ZAI_KEY = 'k1'; $env:RT_ZAIB_KEY = 'k2'; $env:CODEX_CONSULT_ROSTER = $Roster; $env:CODEX_CONSULT_EXE = $Fake
    foreach ($k in $Env.Keys) { Set-Item "env:$k" $Env[$k] } }
function Run { param([string]$Repo, [string]$Ps, [string]$Roster, [string[]]$ArgList, [hashtable]$Env = @{})
    Set-Env $Roster $Env
    Push-Location $Repo
    $p = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    $out = & $psExe -NoProfile -ExecutionPolicy Bypass -File $Ps @ArgList 2>&1
    $code = $LASTEXITCODE
    $ErrorActionPreference = $p
    Pop-Location
    $text = (($out | ForEach-Object { "$_" }) -join "`n")
    $json = $null; try { $json = $text | ConvertFrom-Json } catch { }
    return [pscustomobject]@{ Code = $code; Out = $text; Json = $json; First = (($text -split "`n") | Select-Object -First 1) } }
function Consult { param([string]$Repo, [string]$Roster, [string[]]$ArgList, [hashtable]$Env = @{}) Run $Repo $consultPs $Roster (@('-Task', 't', '-CodexExe', $Fake) + $ArgList) $Env }
function Providers { param([string]$Repo, [string]$Roster, [string[]]$ArgList = @(), [hashtable]$Env = @{}) Run $Repo $providersPs $Roster $ArgList $Env }
function Row { param($Json, [string]$Name) return @($Json | Where-Object { $_.name -ceq $Name })[0] }
function Ledger { param([string]$Repo) $f = Join-Path $Repo '.collab\t\sessions.json'; if (-not (Test-Path $f)) { return @() }; return @(([IO.File]::ReadAllText($f, $u8) | ConvertFrom-Json -DateKind String).codex.consults) }
function Line { param([string]$Text, [string]$Prefix) return @(($Text -split "`n") | Where-Object { $_.StartsWith($Prefix) })[0] }

# ---- E5: a usage limit on ZAI (plan zai) marks ZAIB (plan zai) out, machine-wide too
$hE5 = Join-Path $Work 'health-e5.json'
$rA = New-Repo 'plan-a'
$xa = Consult $rA $rosterPlan @('-Provider', 'ZAI', '-Model', 'glm-5.3', '-Prompt', 'x') @{ FAKE_CODEX_REPLY = $advise; FAKE_CODEX_FAIL_EVENT = $limitMsg; CODEX_CONSULT_HEALTH = $hE5 }
$ea = @(Ledger $rA)[-1]
Check 'E5' 'the ZAI run fails with class quota and a retry_after (3 days ahead); the machine-wide file holds the quota record' ($xa.Code -ne 0 -and $ea.provider_failure.class -eq 'quota' -and [string]$ea.provider_failure.retry_after -match '^\d{4}-\d\d-\d\dT' -and (Test-Path $hE5) -and ([IO.File]::ReadAllText($hE5) -match '"class":\s*"quota"')) "$($xa.First) | $($ea.provider_failure.class) $($ea.provider_failure.retry_after)"
$l = Providers $rA $rosterPlan @('-Json') @{ CODEX_CONSULT_HEALTH = $hE5 }
$ls = Providers $rA $rosterPlan @('-Short') @{ CODEX_CONSULT_HEALTH = $hE5 }
$lt = Providers $rA $rosterPlan @() @{ CODEX_CONSULT_HEALTH = $hE5 }
$cB = Row $l.Json 'ZAIB'; $cA = Row $l.Json 'ZAI'
Check 'E5' 'listing: ZAIB "unavailable (plan zai (usage limit on ZAI until <iso>))", ZAI its own "usage limit until"; -Short "ZAIB :: glm-5.3 (plan zai (usage limit on ZAI until ..."; the walk skips both and would select openai' ([string]$cB.verdict -match '^unavailable \(plan zai \(usage limit on ZAI until \d{4}-\d\d-\d\dT' -and [string]$cA.verdict -match '^unavailable \(usage limit until ' -and $ls.Out -match 'ZAIB :: glm-5\.3 \(plan zai \(usage limit on ZAI until ' -and $lt.Out -match 'would select openai :: gpt-5\.1 \(skipped: ZAI :: glm-5\.3 \(usage limit until [^)]*\), ZAIB :: glm-5\.3 \(plan zai \(usage limit on ZAI until ') "$($cB.verdict) | $($cA.verdict) | $($ls.Out) | $(Line $lt.Out 'roster')"
$x6 = Consult $rA $rosterPlan @('-Provider', 'ZAIB', '-Model', 'glm-5.3', '-Prompt', 'x') @{ FAKE_CODEX_REPLY = $advise; CODEX_CONSULT_HEALTH = $hE5 }
Check 'E5' 'a direct run of the plan''s other route is refused before anything starts: "provider ZAIB is not usable: its plan zai hit a usage limit on ZAI at ... that lasts until ...; nothing was started (pass -SkipPreflight to launch anyway)"; no ledger entry' ($x6.Code -eq 1 -and $x6.Out -match 'provider ZAIB is not usable: its plan zai hit a usage limit on ZAI at .* that lasts until .*; nothing was started \(pass -SkipPreflight to launch anyway\)' -and @(Ledger $rA).Count -eq 1) $x6.First
$d6 = Consult $rA $rosterPlan @('-Provider', 'ZAIB', '-Model', 'glm-5.3', '-Prompt', 'x', '-DryRun') @{ CODEX_CONSULT_HEALTH = $hE5 }
Check 'E5' 'the dry run of that route reports the plan verdict and exits 0' ($d6.Code -eq 0 -and $d6.Out -match '(?m)^preflight\s*:\s*unavailable \(plan zai \(usage limit on ZAI until ') (Line $d6.Out 'preflight')
$w6 = Consult $rA $rosterPlan @('-Prompt', 'x', '-ReplyName', 'walk') @{ FAKE_CODEX_REPLY = $advise; CODEX_CONSULT_HEALTH = $hE5 }
$ew = @(Ledger $rA)[-1]
Check 'E5' 'a roster walk run takes openai (ZAI and ZAIB skipped, the plan reason recorded)' ($w6.Code -eq 0 -and $ew.reviewer.provider -eq 'openai' -and (@($ew.roster.skipped | ForEach-Object { [string]$_.reason }) -join ' ; ') -match 'plan zai \(usage limit on ZAI until ') "$($w6.First) | $(@($ew.roster.skipped | ForEach-Object { "$($_.provider): $($_.reason)" }) -join ' ; ')"
$rB = New-Repo 'plan-b'
$lb = Providers $rB $rosterPlan @('-Json') @{ CODEX_CONSULT_HEALTH = $hE5 }
Check 'E5' 'ANOTHER repository (no ledger) sees ZAI out and ZAIB out through the plan from the machine-wide file' ([string](Row $lb.Json 'ZAI').verdict -match '^unavailable \(usage limit until ' -and [string](Row $lb.Json 'ZAIB').verdict -match '^unavailable \(plan zai \(usage limit on ZAI until ') "$((Row $lb.Json 'ZAI').verdict) | $((Row $lb.Json 'ZAIB').verdict)"
$ln = Providers $rA $rosterNoPlan @('-Json') @{ CODEX_CONSULT_HEALTH = $hE5 }
Check 'E5' 'without a plan the usage limit stays on its route: ZAIB available, ZAI out' ((Row $ln.Json 'ZAIB').verdict -eq 'available' -and [string](Row $ln.Json 'ZAI').verdict -match '^unavailable \(usage limit until ') "$((Row $ln.Json 'ZAIB').verdict)"
$rC = New-Repo 'plan-c'
$xc = Consult $rC $rosterPlan @('-Provider', 'ZAIB', '-Model', 'glm-5.3', '-Prompt', 'x') @{ FAKE_CODEX_REPLY = $advise; FAKE_CODEX_FAIL_EVENT = 'invalid api key (401 Unauthorized)' }
$lc = Providers $rC $rosterPlan @('-Json')
Check 'E5' 'an AUTH failure on ZAIB stays with its route: ZAIB "unavailable (auth failed ...)", ZAI available' ($xc.Code -ne 0 -and [string](Row $lc.Json 'ZAIB').verdict -match '^unavailable \(auth failed' -and (Row $lc.Json 'ZAI').verdict -eq 'available') "$((Row $lc.Json 'ZAIB').verdict) | $((Row $lc.Json 'ZAI').verdict)"
$rD = New-Repo 'plan-d'
$xd = Consult $rD $rosterPlan @('-Provider', 'ZAIB', '-Model', 'glm-5.3', '-Prompt', 'x') @{ FAKE_CODEX_REPLY = $advise; FAKE_CODEX_FAIL_EVENT = $limitMsg }
$ld = Providers $rD $rosterPlan @('-Json')
$yd = Consult $rD $rosterPlan @('-Provider', 'ZAI', '-Model', 'glm-5.3', '-Prompt', 'x', '-SkipPreflight', '-ReplyName', 'skip') @{ FAKE_CODEX_REPLY = $advise }
$ld2 = Providers $rD $rosterPlan @('-Json')
Check 'E5' 'both ways: a limit on ZAIB marks ZAI out ("plan zai (usage limit on ZAIB until ..."); a usable reply on ZAI AFTER it clears the plan for ZAI (one record set) while ZAIB keeps its own limit' ([string](Row $ld.Json 'ZAI').verdict -match '^unavailable \(plan zai \(usage limit on ZAIB until ' -and $yd.Code -eq 0 -and (Row $ld2.Json 'ZAI').verdict -eq 'available' -and [string](Row $ld2.Json 'ZAIB').verdict -match '^unavailable \(usage limit until ') "$((Row $ld.Json 'ZAI').verdict) | $($yd.First) | $((Row $ld2.Json 'ZAI').verdict) | $((Row $ld2.Json 'ZAIB').verdict)"

# ---- E7 through the bridge: the plan serializes its two routes in a panel
$rP = New-Repo 'plan-panel'
$pd = Consult $rP $rosterPlan @('-Panel', '-PanelAll', '-Prompt', 'x', '-DryRun')
$pp = Consult $rP $rosterPar @('-Panel', '-PanelAll', '-Prompt', 'x', '-DryRun')
$pn = Consult $rP $rosterNoPlan @('-Panel', '-PanelAll', '-Prompt', 'x', '-DryRun')
Check 'E7' 'a dry-run panel of ZAI, ZAIB (plan zai, two endpoints) and openai plans "at most 2 at a time"; "parallel": {"zai": 2} -> "at once"; without the plan "at once"' ($pd.Code -eq 0 -and $pd.Out -match 'at most 2 at a time' -and $pp.Code -eq 0 -and $pp.Out -match '(?m)^Concurrency: at once' -and $pn.Out -match '(?m)^Concurrency: at once') "$(Line $pd.Out 'Concurrency') | $(Line $pp.Out 'Concurrency') | $(Line $pn.Out 'Concurrency')"

# ---- E16: two repositories, one machine-wide file: E holds a ZAI run (plan zai); F's panel member ZAIB waits
$hE16 = Join-Path $Work 'health-e16.json'
$rE = New-Repo 'e16-hold'
$rF = New-Repo 'e16-panel'
$rosterE = Write-File 'roster-e16-e.json' ('{"roster_version":1,"reviewers":[' + $zai + ']}')
$rosterF = Write-File 'roster-e16-f.json' ('{"roster_version":1,"reviewers":[' + $zaib + ',' + $oai + ']}')
Set-Env $rosterE @{ FAKE_CODEX_REPLY = $advise; FAKE_CODEX_DELAY_MS = '15000'; CODEX_CONSULT_HEALTH = $hE16 }
$holdOut = Join-Path $Work 'hold.out'
$holdArgs = (@('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $consultPs, '-Task', 't', '-CodexExe', $Fake, '-Provider', 'ZAI', '-Prompt', 'x', '-ReplyName', 'hold') | ForEach-Object { if ([string]$_ -match '[\s"]') { '"' + ([string]$_ -replace '"', '\"') + '"' } else { [string]$_ } }) -join ' '
$hold = Start-Process -FilePath $psExe -ArgumentList $holdArgs -WorkingDirectory $rE -NoNewWindow -PassThru -RedirectStandardOutput $holdOut -RedirectStandardError "$holdOut.err"
$rowE = $null
$sw = [Diagnostics.Stopwatch]::StartNew()
while ($null -eq $rowE -and $sw.Elapsed.TotalSeconds -lt 40) {
    try { $rowE = @(@(([IO.File]::ReadAllText($hE16, $u8) | ConvertFrom-Json).running) | Where-Object { $_.label -eq 'ZAI' })[0] } catch { $rowE = $null }
    if ($null -eq $rowE) { Start-Sleep -Milliseconds 250 }
}
$pF = Consult $rF $rosterF @('-Panel', '-PanelAll', '-Prompt', 'x', '-ReplyName', 'pw') @{ FAKE_CODEX_REPLY = $advise; CODEX_CONSULT_HEALTH = $hE16 }
$holdDone = $hold.WaitForExit(90000)
$lE = @(Ledger $rE)[-1]
$lF = @(Ledger $rF | Where-Object { $_.reviewer.provider -eq 'ZAIB' })[0]
$eFin = [DateTimeOffset]::Parse([string]$lE.finished_at)
$fStart = [DateTimeOffset]::Parse([string]$lF.when)
$waitLine = [string](Line $pF.Out '  panel member 1 of 2 waits:')
$after = $null; try { $after = [IO.File]::ReadAllText($hE16, $u8) | ConvertFrom-Json } catch { }
Check 'E16' 'repository E''s held ZAI run holds a running row WITH plan zai; F''s member ZAIB (plan zai, another endpoint, limit 1) waits: "panel member 1 of 2 waits: 1 run(s) elsewhere on this machine use its plan zai (parallel limit 1): ZAI in <E> task t handoff 01 (plan zai, pid N)" and starts only after E finished; openai does not wait; all usable; running[] empty afterwards' ($null -ne $rowE -and $rowE.plan -eq 'zai' -and $holdDone -and $pF.Code -eq 0 -and $waitLine -match '^  panel member 1 of 2 waits: 1 run\(s\) elsewhere on this machine use its plan zai \(parallel limit 1\): ZAI in \S*e16-hold task t handoff 01 \(plan zai, pid \d+\)$' -and $lE.bridge_outcome -eq 'usable reply' -and $lF.bridge_outcome -eq 'usable reply' -and @(Ledger $rF).Count -eq 2 -and $fStart -ge $eFin.AddSeconds(-1) -and -not ($pF.Out -match 'panel member 2 of 2 waits') -and @($after.running).Count -eq 0) "row plan '$(if ($rowE) { $rowE.plan })' | $waitLine | E finished $($lE.finished_at), F ZAIB when $($lF.when) | running after $(@($after.running).Count) | $($pF.First)"

Write-Host ("plan-scenario: {0} passed, {1} failure(s)" -f $script:pass, $script:fail)
