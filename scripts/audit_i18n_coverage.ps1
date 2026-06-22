param(
  [string]$SourceRoot = "src",
  [string]$JsonOut = "docs/audit/i18n_coverage.json",
  [string]$MarkdownOut = "docs/audit/i18n_coverage.md",
  [ValidateSet("core","prod")]
  [string]$Scope = "core",
  [string[]]$Kinds = @("stdout", "log", "error_attr"),
  [string[]]$ExcludePatterns = @(
    "/test.rs",
    "/tests/",
    "/showcase.rs",
    "/bin/test_",
    "/bin/pipeline_bench.rs",
    "/bin/log_miner.rs",
    "/bin/tree_dict_experiment.rs"
  )
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Ensure-Dir([string]$path) {
  $dir = Split-Path -Parent $path
  if (-not [string]::IsNullOrWhiteSpace($dir) -and -not (Test-Path -LiteralPath $dir)) {
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
  }
}

function Normalize-Path([string]$path) {
  return ($path -replace '\\', '/')
}

function Is-Excluded([string]$normalizedPath, [string[]]$patterns) {
  foreach ($p in $patterns) {
    if ($normalizedPath.ToLowerInvariant().Contains($p.ToLowerInvariant())) {
      return $true
    }
  }
  return $false
}

function Is-I18nLine([string]$line) {
  if ($line -match '\bt!\s*\(') { return $true }
  if ($line -match '\bt\s*\(') { return $true }
  if ($line -match '\bi18n::t\s*\(') { return $true }
  if ($line -match '\bt1\s*\(') { return $true }
  if ($line -match '\bt2\s*\(') { return $true }
  if ($line -match '\bt_en\s*\(') { return $true }
  if ($line -match '\bt_zh\s*\(') { return $true }
  if ($line -match 'render_user_facing_terminal_message\s*\(') { return $true }
  return $false
}

function Is-InScope([string]$normalizedPath, [string]$scope) {
  $p = $normalizedPath.ToLowerInvariant()
  if ($scope -eq "core") {
    return $p.StartsWith("src/core/")
  }
  if ($scope -eq "prod") {
    if ($p.StartsWith("src/core/")) { return $true }
    if ($p.StartsWith("src/cli/")) { return $true }
    if ($p -eq "src/plugins/vcs_plugin/methods/core_logic.rs") { return $true }
    if ($p -eq "src/main.rs") { return $true }
    if ($p -eq "src/bin/tokenslim-server.rs") { return $true }
    return $false
  }
  return $false
}

function Has-HumanLiteral([string]$line) {
  $regex = [regex]'"([^"\\]|\\.)*"'
  $matches = $regex.Matches($line)
  foreach ($m in $matches) {
    $raw = $m.Value.Trim('"')
    $noPlaceholders = [regex]::Replace($raw, '\{[^}]*\}', '')
    if ($noPlaceholders -match '^[A-Z0-9_:\-]+$') {
      continue
    }
    if ($noPlaceholders -match '[A-Za-z\u4e00-\u9fff]') {
      return $true
    }
  }
  return $false
}

function To-Rel([string]$full) {
  $root = (Resolve-Path ".").Path
  $prefix = $root + [System.IO.Path]::DirectorySeparatorChar
  if ($full.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    return $full.Substring($prefix.Length)
  }
  return $full
}

Ensure-Dir $JsonOut
Ensure-Dir $MarkdownOut

$rootResolved = (Resolve-Path $SourceRoot).Path
$uniqueFiles = @(Get-ChildItem -Path $rootResolved -File -Recurse -Filter *.rs | Select-Object -ExpandProperty FullName | Sort-Object -Unique)

$findings = New-Object System.Collections.Generic.List[object]
$scanned = 0
foreach ($file in $uniqueFiles) {
  $rel = To-Rel $file
  $normalized = Normalize-Path $rel
  if (-not (Is-InScope $normalized $Scope)) { continue }
  if (Is-Excluded $normalized $ExcludePatterns) { continue }
  if (-not $normalized.EndsWith(".rs")) { continue }
  $scanned++
  $lines = Get-Content -LiteralPath $file -Encoding UTF8
  for ($i = 0; $i -lt $lines.Count; $i++) {
    $lineNo = $i + 1
    $line = [string]$lines[$i]
    $trim = $line.Trim()
    if ([string]::IsNullOrWhiteSpace($trim)) { continue }

    $kind = $null
    if ($trim -match 'println!\s*\(' -or $trim -match 'eprintln!\s*\(') {
      $kind = "stdout"
    } elseif ($trim -match 'log::(info|warn|error|debug|trace)!\s*\(') {
      $kind = "log"
    } elseif ($trim -match '#\[error\("') {
      $kind = "error_attr"
    } else {
      continue
    }
    if (-not ($Kinds -contains $kind)) { continue }

    if (Is-I18nLine $trim) { continue }

    # 仅记录疑似硬编码字符串的场景（避免误报如 "{}" / "{:?}" 这类占位输出）
    if ($trim -notmatch '"') { continue }
    if (-not (Has-HumanLiteral $trim)) { continue }

    $findings.Add([PSCustomObject]@{
      file = $normalized
      line = $lineNo
      kind = $kind
      text = $trim
    }) | Out-Null
  }
}

$byKind = @{
  stdout = [int]@($findings | Where-Object { $_.kind -eq "stdout" }).Count
  log = [int]@($findings | Where-Object { $_.kind -eq "log" }).Count
  error_attr = [int]@($findings | Where-Object { $_.kind -eq "error_attr" }).Count
}
$byFile = @()
if ($findings.Count -gt 0) {
  $byFile = @($findings | Group-Object file | Sort-Object Count -Descending | ForEach-Object {
    [PSCustomObject]@{
      file = $_.Name
      count = [int]$_.Count
    }
  })
}

$report = @{}
$report["generated_at"] = (Get-Date).ToString("s")
$report["source_root"] = $SourceRoot
$report["scanned_files"] = $scanned
$report["scope"] = $Scope
$report["exclude_patterns"] = $ExcludePatterns
$report["hardcoded_total"] = [int]$findings.Count
$report["by_kind"] = $byKind
$report["by_file"] = $byFile
$findingsOut = New-Object System.Collections.ArrayList
foreach ($f in $findings) { [void]$findingsOut.Add($f) }
$report["findings"] = $findingsOut

$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $JsonOut -Encoding UTF8

$md = New-Object System.Collections.Generic.List[string]
$md.Add("# I18N Coverage Audit")
$md.Add("")
$md.Add("- generated_at: $($report["generated_at"])")
$md.Add("- scanned_files: $($report["scanned_files"])")
$md.Add("- hardcoded_total: $($report["hardcoded_total"])")
$md.Add("- by_kind: stdout=$($report["by_kind"].stdout), log=$($report["by_kind"].log), error_attr=$($report["by_kind"].error_attr)")
$md.Add("")
$md.Add("## Top Files")
$md.Add("")
$md.Add("| file | count |")
$md.Add("| ---- | ----: |")
foreach ($item in ($byFile | Select-Object -First 20)) {
  $md.Add("| $($item.file) | $($item.count) |")
}
$md.Add("")
$md.Add("## Findings")
$md.Add("")
$md.Add("| kind | file | line | text |")
$md.Add("| ---- | ---- | ---: | ---- |")
foreach ($f in $findings) {
  $safeText = ([string]$f.text).Replace("|", "\|")
  $md.Add("| $($f.kind) | $($f.file) | $($f.line) | $safeText |")
}

Set-Content -LiteralPath $MarkdownOut -Value $md -Encoding UTF8

Write-Output "i18n_audit_json=$JsonOut"
Write-Output "i18n_audit_md=$MarkdownOut"
Write-Output "scanned_files=$($report.scanned_files)"
Write-Output "hardcoded_total=$($report.hardcoded_total)"
Write-Output "hardcoded_stdout=$($report.by_kind.stdout)"
Write-Output "hardcoded_log=$($report.by_kind.log)"
Write-Output "hardcoded_error_attr=$($report.by_kind.error_attr)"
