param(
  [string]$AuditRoot = "docs/audit",
  [string]$ArchiveRoot = "docs/archive/audit_artifacts",
  [int]$KeepVersions = 3,
  [string[]]$IncludePlugins = @(),
  [string[]]$ExcludePlugins = @(),
  [switch]$ApplyCleanup,
  [switch]$JsonMin
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Ensure-Directory {
  param([string]$Path)
  if (-not (Test-Path -LiteralPath $Path)) {
    New-Item -ItemType Directory -Path $Path | Out-Null
  }
}

function Add-Candidate {
  param(
    [System.Collections.Generic.List[object]]$List,
    [string]$Path,
    [string]$Version,
    [string]$Kind
  )
  if (Test-Path -LiteralPath $Path) {
    $List.Add([PSCustomObject]@{
      path = $Path
      version = $Version
      kind = $Kind
    }) | Out-Null
  }
}

function Expand-PluginList {
  param([string[]]$InputList)
  $expanded = New-Object System.Collections.Generic.List[string]
  foreach ($item in $InputList) {
    if ([string]::IsNullOrWhiteSpace($item)) { continue }
    $parts = $item -split '[,;]'
    foreach ($part in $parts) {
      $name = $part.Trim()
      if (-not [string]::IsNullOrWhiteSpace($name)) {
        $expanded.Add($name) | Out-Null
      }
    }
  }
  return @($expanded)
}

$repoRoot = (Get-Location).Path
$auditRootPath = [System.IO.Path]::GetFullPath((Join-Path $repoRoot $AuditRoot))
$archiveRootPath = [System.IO.Path]::GetFullPath((Join-Path $repoRoot $ArchiveRoot))

if (-not (Test-Path -LiteralPath $auditRootPath)) {
  throw "audit root not found: $auditRootPath"
}

Ensure-Directory -Path $archiveRootPath

$pluginDirs = Get-ChildItem -LiteralPath $auditRootPath -Directory |
  Where-Object {
    (Test-Path -LiteralPath (Join-Path $_.FullName "frozen_cases.json")) -or
    (Get-ChildItem -LiteralPath $_.FullName -Filter "*.latest.json" -File -ErrorAction SilentlyContinue | Measure-Object).Count -gt 0
  } |
  Sort-Object Name

if ($IncludePlugins.Count -gt 0) {
  $IncludePlugins = Expand-PluginList -InputList $IncludePlugins
  $includeSet = @{}
  foreach ($name in $IncludePlugins) {
    if (-not [string]::IsNullOrWhiteSpace($name)) {
      $includeSet[$name.ToLowerInvariant()] = $true
    }
  }
  $pluginDirs = @($pluginDirs | Where-Object { $includeSet.ContainsKey($_.Name.ToLowerInvariant()) })
}

if ($ExcludePlugins.Count -gt 0) {
  $ExcludePlugins = Expand-PluginList -InputList $ExcludePlugins
  $excludeSet = @{}
  foreach ($name in $ExcludePlugins) {
    if (-not [string]::IsNullOrWhiteSpace($name)) {
      $excludeSet[$name.ToLowerInvariant()] = $true
    }
  }
  $pluginDirs = @($pluginDirs | Where-Object { -not $excludeSet.ContainsKey($_.Name.ToLowerInvariant()) })
}

$results = New-Object System.Collections.Generic.List[object]
$totalArchiveCandidates = 0
$totalMoved = 0

foreach ($dir in $pluginDirs) {
  $plugin = $dir.Name
  $latestFile = Join-Path $dir.FullName "$plugin.latest.json"
  $stateFile = Join-Path $dir.FullName "audit_state.json"
  $frozenFile = Join-Path $dir.FullName "frozen_cases.json"
  $casesDir = Join-Path $dir.FullName "cases"

  $requiredMissing = New-Object System.Collections.Generic.List[string]
  if (-not (Test-Path -LiteralPath $latestFile)) { $requiredMissing.Add("latest_json") | Out-Null }
  if (-not (Test-Path -LiteralPath $stateFile)) { $requiredMissing.Add("audit_state") | Out-Null }
  if (-not (Test-Path -LiteralPath $frozenFile)) { $requiredMissing.Add("frozen_cases") | Out-Null }
  if (-not (Test-Path -LiteralPath $casesDir)) { $requiredMissing.Add("cases_dir") | Out-Null }

  $snapshotFiles = Get-ChildItem -LiteralPath $dir.FullName -File -Filter "$plugin.*.json" |
    Where-Object { $_.Name -ne "$plugin.latest.json" } |
    Sort-Object LastWriteTime -Descending

  $keptVersions = @($snapshotFiles | Select-Object -First $KeepVersions | ForEach-Object {
      $_.BaseName.Substring($plugin.Length + 1)
    })
  $staleVersions = @($snapshotFiles | Select-Object -Skip $KeepVersions | ForEach-Object {
      $_.BaseName.Substring($plugin.Length + 1)
    })

  $archiveCandidates = New-Object System.Collections.Generic.List[object]
  foreach ($version in $staleVersions) {
    Add-Candidate -List $archiveCandidates -Path (Join-Path $dir.FullName "$plugin.$version.json") -Version $version -Kind "json"
    Add-Candidate -List $archiveCandidates -Path (Join-Path $dir.FullName "$plugin.$version.csv") -Version $version -Kind "csv"
    Add-Candidate -List $archiveCandidates -Path (Join-Path $dir.FullName "$plugin.$version.diff.md") -Version $version -Kind "diff"
  }

  $moved = 0
  if ($ApplyCleanup -and $archiveCandidates.Count -gt 0) {
    $pluginArchiveDir = Join-Path $archiveRootPath $plugin
    Ensure-Directory -Path $pluginArchiveDir
    foreach ($candidate in $archiveCandidates) {
      $target = Join-Path $pluginArchiveDir ([System.IO.Path]::GetFileName($candidate.path))
      if (Test-Path -LiteralPath $target) {
        Remove-Item -LiteralPath $target -Force
      }
      Move-Item -LiteralPath $candidate.path -Destination $target -Force
      $moved++
    }
  }

  $totalArchiveCandidates += $archiveCandidates.Count
  $totalMoved += $moved

  $results.Add([PSCustomObject]@{
    plugin = $plugin
    snapshots_total = $snapshotFiles.Count
    kept_versions = $keptVersions
    stale_versions = $staleVersions
    archive_candidates = $archiveCandidates.Count
    moved = $moved
    required_missing = @($requiredMissing)
  }) | Out-Null
}

$generatedAt = (Get-Date).ToString("s")
$pluginsArray = @($results.ToArray())
$reportObj = [ordered]@{
  generated_at = $generatedAt
  audit_root = $auditRootPath
  archive_root = $archiveRootPath
  keep_versions = $KeepVersions
  apply_cleanup = [bool]$ApplyCleanup
  plugin_count = $results.Count
  total_archive_candidates = $totalArchiveCandidates
  total_moved = $totalMoved
  plugins = $pluginsArray
}
$reportObj = [PSCustomObject]$reportObj

$jsonPath = Join-Path $auditRootPath "audit_artifact_governance.json"
$mdPath = Join-Path $auditRootPath "audit_artifact_governance.md"

$reportObj | ConvertTo-Json -Depth 8 | Set-Content -Path $jsonPath -Encoding UTF8

$md = New-Object System.Collections.Generic.List[string]
$md.Add("# Audit Artifact Governance") | Out-Null
$md.Add("") | Out-Null
$md.Add("- generated_at: $generatedAt") | Out-Null
$md.Add("- keep_versions: $KeepVersions") | Out-Null
$md.Add("- apply_cleanup: $([bool]$ApplyCleanup)") | Out-Null
$md.Add("- plugin_count: $($results.Count)") | Out-Null
$md.Add("- total_archive_candidates: $totalArchiveCandidates") | Out-Null
$md.Add("- total_moved: $totalMoved") | Out-Null
$md.Add("") | Out-Null
$md.Add("| plugin | snapshots_total | archive_candidates | moved | required_missing |") | Out-Null
$md.Add("| --- | ---: | ---: | ---: | --- |") | Out-Null
foreach ($r in $results) {
  $missing = if (@($r.required_missing).Count -eq 0) { "-" } else { (@($r.required_missing) -join ",") }
  $md.Add("| $($r.plugin) | $($r.snapshots_total) | $($r.archive_candidates) | $($r.moved) | $missing |") | Out-Null
}
$md.Add("") | Out-Null
$md.Add("## Policy") | Out-Null
$md.Add("- Keep latest $KeepVersions version snapshots (<plugin>.<version>.json|csv|diff.md) in docs/audit/<plugin>/.") | Out-Null
$md.Add("- Move older snapshots to $ArchiveRoot/<plugin>/ when -ApplyCleanup is used.") | Out-Null
$md.Add("- Required active artifacts: <plugin>.latest.json, audit_state.json, frozen_cases.json, cases/.") | Out-Null

$md | Set-Content -Path $mdPath -Encoding UTF8

Write-Output "audit_artifact_governance_report_json=$jsonPath"
Write-Output "audit_artifact_governance_report_md=$mdPath"
Write-Output "plugin_count=$($results.Count)"
Write-Output "total_archive_candidates=$totalArchiveCandidates"
Write-Output "total_moved=$totalMoved"
if ($JsonMin) {
  $min = [ordered]@{
    generated_at = $generatedAt
    plugin_count = $results.Count
    keep_versions = $KeepVersions
    apply_cleanup = [bool]$ApplyCleanup
    total_archive_candidates = $totalArchiveCandidates
    total_moved = $totalMoved
  }
  Write-Output ("json_min=" + ($min | ConvertTo-Json -Compress))
}
