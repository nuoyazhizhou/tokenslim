param(
  [string]$RepoRoot = ".",
  [ValidateSet("debug", "release")]
  [string]$Profile = "debug",
  [switch]$SyncBin,
  [switch]$RequireBinSync,
  [int]$ServerPort = 10086
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-Contains {
  param(
    [string]$Text,
    [string]$Needle,
    [string]$Message
  )
  if ($Text -notmatch [regex]::Escape($Needle)) {
    throw "ASSERT FAILED: $Message (missing: $Needle)"
  }
}

function Assert-AnyContains {
  param(
    [string]$Text,
    [string[]]$Needles,
    [string]$Message
  )
  foreach ($needle in $Needles) {
    if ($Text -match [regex]::Escape($needle)) {
      return
    }
  }
  throw "ASSERT FAILED: $Message (none matched: $($Needles -join ', '))"
}

function Run-Capture {
  param(
    [string]$Exe,
    [string[]]$CmdArgs
  )
  $stdoutFile = [System.IO.Path]::GetTempFileName()
  $stderrFile = [System.IO.Path]::GetTempFileName()
  try {
    if ($null -eq $CmdArgs -or $CmdArgs.Count -eq 0) {
      $p = Start-Process -FilePath $Exe -NoNewWindow -PassThru -Wait `
        -RedirectStandardOutput $stdoutFile -RedirectStandardError $stderrFile
    } else {
      $p = Start-Process -FilePath $Exe -ArgumentList $CmdArgs -NoNewWindow -PassThru -Wait `
        -RedirectStandardOutput $stdoutFile -RedirectStandardError $stderrFile
    }
    $stdout = Get-Content -LiteralPath $stdoutFile -Raw -ErrorAction SilentlyContinue
    $stderr = Get-Content -LiteralPath $stderrFile -Raw -ErrorAction SilentlyContinue
    $combined = ([string]$stdout + "`n" + [string]$stderr)
  return [PSCustomObject]@{
    ExitCode = $p.ExitCode
    Stdout = [string]$stdout
    Stderr = [string]$stderr
    Combined = [string]$combined
  }
  } finally {
    Remove-Item -LiteralPath $stdoutFile, $stderrFile -Force -ErrorAction SilentlyContinue
  }
}

function Wait-HttpReady {
  param(
    [string]$Url,
    [int]$MaxRetry = 25
  )
  for ($i = 0; $i -lt $MaxRetry; $i++) {
    try {
      Invoke-RestMethod -Uri $Url -TimeoutSec 2 | Out-Null
      return $true
    } catch {
      Start-Sleep -Milliseconds 300
    }
  }
  return $false
}

$root = (Resolve-Path $RepoRoot).Path
Push-Location $root
try {
  Write-Host "== build tokenslim ($Profile)"
  if ($Profile -eq "release") {
    & cargo build --bin tokenslim --release
  } else {
    & cargo build --bin tokenslim
  }
  if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

  Write-Host "== build tokenslim-server ($Profile)"
  if ($Profile -eq "release") {
    & cargo build --bin tokenslim-server --release
  } else {
    & cargo build --bin tokenslim-server
  }
  if ($LASTEXITCODE -ne 0) { throw "cargo build tokenslim-server failed" }

  $targetExe = if ($Profile -eq "release") {
    Join-Path $root "target/release/tokenslim.exe"
  } else {
    Join-Path $root "target/debug/tokenslim.exe"
  }
  $binExe = Join-Path $root "bin/tokenslim.exe"
  $serverExe = if ($Profile -eq "release") {
    Join-Path $root "target/release/tokenslim-server.exe"
  } else {
    Join-Path $root "target/debug/tokenslim-server.exe"
  }

  if (-not (Test-Path -LiteralPath $targetExe)) {
    throw "built binary missing: $targetExe"
  }

  if ($SyncBin) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $binExe) | Out-Null
    Copy-Item -LiteralPath $targetExe -Destination $binExe -Force
    Write-Host "sync_bin=ok path=$binExe"
  }

  if ($RequireBinSync) {
    if (-not (Test-Path -LiteralPath $binExe)) {
      throw "bin binary missing while -RequireBinSync enabled: $binExe"
    }
    $targetHash = (Get-FileHash -LiteralPath $targetExe -Algorithm SHA256).Hash
    $binHash = (Get-FileHash -LiteralPath $binExe -Algorithm SHA256).Hash
    if ($targetHash -ne $binHash) {
      throw "bin/target hash mismatch. Run with -SyncBin to align publish binary."
    }
    Write-Host "bin_sync=ok"
  }

  Write-Host "== smoke: help quick usage"
  $r0 = Run-Capture -Exe $targetExe -CmdArgs @("--help")
  Assert-Contains -Text $r0.Combined -Needle "Usage:" -Message "help should show usage"
  Assert-Contains -Text $r0.Combined -Needle "run <command>" -Message "help should show run guidance"

  Write-Host "== smoke: implicit run path"
  $r1 = Run-Capture -Exe $targetExe -CmdArgs @("git", "--version")
  Assert-Contains -Text $r1.Combined -Needle "git version" -Message "implicit run should execute git"

  Write-Host "== smoke: invalid run target"
  $r2 = Run-Capture -Exe $targetExe -CmdArgs @("run", "--gain")
  Assert-Contains -Text $r2.Combined -Needle "E_CLI_RUN_INVALID_TARGET" -Message "run invalid target should emit structured code"

  Write-Host "== smoke: invalid flag"
  $r3 = Run-Capture -Exe $targetExe -CmdArgs @("--badflag")
  Assert-Contains -Text $r3.Combined -Needle "E_CLI_INVALID_ARGS" -Message "invalid flag should emit structured code"

  Write-Host "== smoke: doctor workspace"
  $r4 = Run-Capture -Exe $targetExe -CmdArgs @("workspace", "--format", "llm")
  Assert-AnyContains -Text $r4.Combined -Needles @('"proj"', '"os"', '"repo"') -Message "workspace llm output should contain compact workspace keys"

  Write-Host "== smoke: gain"
  $r5 = Run-Capture -Exe $targetExe -CmdArgs @("gain")
  Assert-AnyContains -Text $r5.Combined -Needles @("TokenSlim", "尚未记录任何压缩执行", "No compression runs recorded yet") -Message "gain should return readable report"

  Write-Host "== smoke: explain route"
  $r6 = Run-Capture -Exe $targetExe -CmdArgs @("run", "--explain-route", "git", "--version")
  Assert-AnyContains -Text $r6.Combined -Needles @("route_plugin=", "run_route") -Message "explain-route should print route diagnostics"

  Write-Host "== smoke: server health + stats"
  if (-not (Test-Path -LiteralPath $serverExe)) {
    throw "server binary missing: $serverExe"
  }
  $serverProc = $null
  try {
    $env:TOKENSLIM_HOST = "127.0.0.1"
    $env:TOKENSLIM_PORT = "$ServerPort"
    $serverProc = Start-Process -FilePath $serverExe -WindowStyle Hidden -PassThru
    $healthUrl = "http://127.0.0.1:$ServerPort/health"
    $statsUrl = "http://127.0.0.1:$ServerPort/stats/aggregate"
    $metricsDetailUrl = "http://127.0.0.1:$ServerPort/metrics/detail"
    if (-not (Wait-HttpReady -Url $healthUrl)) {
      throw "server health endpoint not ready: $healthUrl"
    }
    $health = Invoke-RestMethod -Uri $healthUrl -TimeoutSec 3
    if ($health.status -ne "UP") {
      throw "server health status != UP"
    }
    $stats = Invoke-RestMethod -Uri $statsUrl -TimeoutSec 3
    if ($null -eq $stats.total_commands) {
      throw "server stats aggregate missing total_commands"
    }
    $detail = Invoke-RestMethod -Uri $metricsDetailUrl -TimeoutSec 3
    if ($null -eq $detail.module_timings_ms) {
      throw "server metrics detail missing module_timings_ms"
    }
    if ($null -eq $detail.plugin_stats) {
      throw "server metrics detail missing plugin_stats"
    }
  } finally {
    if ($null -ne $serverProc) {
      Stop-Process -Id $serverProc.Id -Force -ErrorAction SilentlyContinue
    }
    Remove-Item Env:TOKENSLIM_HOST -ErrorAction SilentlyContinue
    Remove-Item Env:TOKENSLIM_PORT -ErrorAction SilentlyContinue
  }

  Write-Host "smoke_gate=pass profile=$Profile target=$targetExe"
} finally {
  Pop-Location
}
