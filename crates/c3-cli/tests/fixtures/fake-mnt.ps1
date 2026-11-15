# FAKE engine/npm CLIs for the `c3 maintain` harness. One brain, three roles (FAKE_MNT_ROLE set by
# the tiny .cmd wrapper that calls this): grok, codex, npm. Never calls a real CLI or model. The
# "installed version" of grok/codex lives in a file so an `update` is observable by a later
# `--version`.
#   grok:  --version -> "grok <cur>"; update --check --json -> {latestVersion,...}; update ->
#          write FAKE_GROK_LATEST to FAKE_GROK_VERFILE; update --version <v> -> write <v>;
#          models -> a short list.
#   codex: --version -> "codex-cli <cur>".
#   npm:   view @openai/codex version -> FAKE_NPM_LATEST; view ... time --json -> {latest:date};
#          ls -g @openai/codex -> "@openai/codex@1.0.0"; install -g @openai/codex@<v> -> write <v>.
$ErrorActionPreference = 'Stop'
$u8 = New-Object System.Text.UTF8Encoding($false)
function Out-Line($t) { $b = $u8.GetBytes($t + "`n"); $s = [Console]::OpenStandardOutput(); $s.Write($b, 0, $b.Length); $s.Flush() }
$role = [string]$env:FAKE_MNT_ROLE
$raw = [string]$env:FAKE_MNT_ARGS
$argv = @($raw -split '\s+' | Where-Object { $_ -ne '' })
if ($env:FAKE_MNT_LOG) { try { Add-Content -LiteralPath $env:FAKE_MNT_LOG -Value "$role :: $raw" } catch {} }
# Dump this child's environment variable NAMES (not values) so a test can assert that provider
# secrets were scrubbed before an update/probe child ran.
if ($env:FAKE_MNT_DUMPENV) {
  try {
    $names = (Get-ChildItem Env: | ForEach-Object { $_.Name }) -join "`n"
    Add-Content -LiteralPath $env:FAKE_MNT_DUMPENV -Value "$role :: $raw`n$names`n----"
  } catch {}
}

switch ($role) {
  'grok' {
    $vf = [string]$env:FAKE_GROK_VERFILE
    $cur = if ($vf -and (Test-Path $vf)) { (Get-Content $vf -Raw).Trim() } else { [string]$env:FAKE_GROK_VER }
    # NOTE: the `update` subcommand is handled before the bare `--version` probe, because a rollback
    # is `grok update --version <v>` and that `--version` flag must NOT be read as the version probe.
    if ($argv.Count -ge 1 -and $argv[0] -eq 'update') {
      if ($argv -contains '--check') {
        $latest = [string]$env:FAKE_GROK_LATEST
        $avail = if ($latest -ne $cur) { 'true' } else { 'false' }
        Out-Line ('{"currentVersion":"' + $cur + '","latestVersion":"' + $latest + '","updateAvailable":' + $avail + ',"installer":"internal","channel":"stable"}')
        exit 0
      }
      $vi = [Array]::IndexOf($argv, '--version')
      if ($vi -ge 0 -and $vi + 1 -lt $argv.Count) {
        $to = $argv[$vi + 1]
        $isForward = ($to -eq [string]$env:FAKE_GROK_LATEST)
        # FAKE_GROK_UPDATE_NOOP: a forward update exits 0 without changing the version ("did not
        # take"); FAKE_GROK_ROLLBACK_NOOP: a rollback exits 0 without restoring ("rollback failed").
        if ($isForward -and $env:FAKE_GROK_UPDATE_NOOP) { exit 0 }
        if (-not $isForward -and $env:FAKE_GROK_ROLLBACK_NOOP) { exit 0 }
        if ($vf) { Set-Content -LiteralPath $vf -Value $to -NoNewline }
        # FAKE_GROK_UPDATE_MUTATE_FAIL: a forward update WRITES the new version (mutates the install)
        # but then exits non-zero, modelling an installer that changed the install yet reported failure.
        if ($isForward -and $env:FAKE_GROK_UPDATE_MUTATE_FAIL) { exit 1 }
        exit 0
      }
      if ($vf) { Set-Content -LiteralPath $vf -Value ([string]$env:FAKE_GROK_LATEST) -NoNewline }
      $ue = [string]$env:FAKE_GROK_UPDATE_EXIT; if (-not $ue) { $ue = '0' }
      exit ([int]$ue)
    }
    if ($argv -contains '--version') {
      # FAKE_GROK_SLEEP: the version probe sleeps this many seconds, so a pass spawned by the hook
      # takes measurable wall time (used to prove the hook returns without waiting for the pass).
      if ($env:FAKE_GROK_SLEEP) { Start-Sleep -Seconds ([int]$env:FAKE_GROK_SLEEP) }
      Out-Line "grok $cur"; exit 0
    }
    if ($argv.Count -ge 1 -and $argv[0] -eq 'models') { Out-Line '  * grok-4.7 (default)'; Out-Line '  - grok-4.6'; exit 0 }
    exit 0
  }
  'codex' {
    $vf = [string]$env:FAKE_CODEX_VERFILE
    $cur = if ($vf -and (Test-Path $vf)) { (Get-Content $vf -Raw).Trim() } else { [string]$env:FAKE_CODEX_VER }
    if ($argv -contains '--version') { Out-Line "codex-cli $cur"; exit 0 }
    exit 0
  }
  'npm' {
    if ($argv.Count -ge 1 -and $argv[0] -eq 'view') {
      if ($argv -contains 'time') {
        $l = [string]$env:FAKE_NPM_LATEST; $d = [string]$env:FAKE_NPM_DATE
        Out-Line ('{"' + $l + '":"' + $d + '"}')
        exit 0
      }
      if ($argv -contains 'version') { Out-Line ([string]$env:FAKE_NPM_LATEST); exit 0 }
      exit 0
    }
    if ($argv.Count -ge 1 -and $argv[0] -eq 'ls') { Out-Line '@openai/codex@1.0.0'; exit 0 }
    if ($argv.Count -ge 1 -and $argv[0] -eq 'install') {
      $to = ''
      foreach ($a in $argv) { if ($a -like '@openai/codex@*') { $to = $a.Substring('@openai/codex@'.Length) } }
      if ($env:FAKE_CODEX_VERFILE -and $to) { Set-Content -LiteralPath $env:FAKE_CODEX_VERFILE -Value $to -NoNewline }
      exit 0
    }
    exit 0
  }
  default { exit 0 }
}
