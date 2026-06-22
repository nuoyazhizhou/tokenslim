param(
  [ValidateSet("core", "prod")]
  [string]$Scope = "prod",
  [string]$SourceRoot = "src",
  [string]$JsonOut = "docs/audit/error_literal_guard.json",
  [string]$MarkdownOut = "docs/audit/error_literal_guard.md",
  [string[]]$ExcludePatterns = @(
    "/tests/",
    "/test.rs",
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
  return ($path -replace "\\", "/")
}

function Is-Excluded([string]$normalizedPath, [string[]]$patterns) {
  $candidate = $normalizedPath.ToLowerInvariant()
  foreach ($p in $patterns) {
    if ($candidate.Contains($p.ToLowerInvariant())) {
      return $true
    }
  }
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
    if ($p -eq "src/main.rs") { return $true }
    if ($p -eq "src/bin/tokenslim-server.rs") { return $true }
    if ($p -eq "src/plugins/vcs_plugin/methods/core_logic.rs") { return $true }
    return $false
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

function Count-Braces([string]$line) {
  $opens = ([regex]::Matches($line, "\{")).Count
  $closes = ([regex]::Matches($line, "\}")).Count
  return ($opens - $closes)
}

function Is-StableErrorLiteral([string]$literal) {
  # Stable code format: E_CODE or E_CODE:context
  if ($literal -match '^E_[A-Z0-9_]+(?::.*)?$') { return $true }
  return $false
}

Ensure-Dir $JsonOut
Ensure-Dir $MarkdownOut

$rootResolved = (Resolve-Path $SourceRoot).Path
$files = Get-ChildItem -Path $rootResolved -File -Recurse -Filter *.rs |
  Select-Object -ExpandProperty FullName |
  Sort-Object -Unique

$findings = @()
$scanned = 0

foreach ($file in $files) {
  $rel = To-Rel $file
  $normalized = Normalize-Path $rel
  if (-not (Is-InScope $normalized $Scope)) { continue }
  if (Is-Excluded $normalized $ExcludePatterns) { continue }

  $scanned++
  $lines = Get-Content -LiteralPath $file -Encoding UTF8

  $pendingTestAttr = $false
  $inTestModule = $false
  $testBraceBalance = 0

  for ($i = 0; $i -lt $lines.Count; $i++) {
    $lineNo = $i + 1
    $line = [string]$lines[$i]
    $trim = $line.Trim()

    if ($trim -match '^\s*#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]') {
      $pendingTestAttr = $true
      continue
    }

    if ($pendingTestAttr -and $trim -match '^\s*(pub\s+)?mod\s+\w+\s*\{') {
      $inTestModule = $true
      $testBraceBalance = Count-Braces $line
      $pendingTestAttr = $false
      continue
    }

    if ($pendingTestAttr -and -not [string]::IsNullOrWhiteSpace($trim)) {
      # Allow chained attributes after #[cfg(test)]
      if ($trim -notmatch '^\s*#\s*\[') {
        $pendingTestAttr = $false
      }
    }

    if ($inTestModule) {
      $testBraceBalance += Count-Braces $line
      if ($testBraceBalance -le 0) {
        $inTestModule = $false
      }
      continue
    }

    $expectMatch = [regex]::Match($trim, '\bexpect\s*\(\s*"([^"]+)"\s*\)')
    if ($expectMatch.Success) {
      $lit = $expectMatch.Groups[1].Value
      if (-not (Is-StableErrorLiteral $lit)) {
        $findings += [pscustomobject]@{
          file = $normalized
          line = $lineNo
          kind = "expect_literal"
          literal = $lit
          text = $trim
        }
      }
    }

    $panicMatch = [regex]::Match($trim, '\bpanic!\s*\(\s*"([^"]+)"')
    if ($panicMatch.Success) {
      $lit = $panicMatch.Groups[1].Value
      if (-not (Is-StableErrorLiteral $lit)) {
        $findings += [pscustomobject]@{
          file = $normalized
          line = $lineNo
          kind = "panic_literal"
          literal = $lit
          text = $trim
        }
      }
    }
  }
}

$byKind = @{
  expect_literal = [int]@($findings | Where-Object { $_.kind -eq "expect_literal" }).Count
  panic_literal = [int]@($findings | Where-Object { $_.kind -eq "panic_literal" }).Count
}

$byFile = @()
if ($findings.Count -gt 0) {
  $byFile = $findings |
    Group-Object file |
    Sort-Object Count -Descending |
    ForEach-Object {
      [pscustomobject]@{
        file = $_.Name
        count = [int]$_.Count
      }
    }
}

$report = @{
  generated_at = (Get-Date).ToString("s")
  scope = $Scope
  source_root = $SourceRoot
  scanned_files = $scanned
  exclude_patterns = $ExcludePatterns
  non_stable_literal_total = [int]$findings.Count
  by_kind = $byKind
  by_file = @($byFile)
  findings = @($findings)
}

$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $JsonOut -Encoding UTF8

$md = @()
$md += "# Error Literal Guard Audit"
$md += ""
$md += "- generated_at: $($report.generated_at)"
$md += "- scope: $($report.scope)"
$md += "- scanned_files: $($report.scanned_files)"
$md += "- non_stable_literal_total: $($report.non_stable_literal_total)"
$md += "- by_kind: expect_literal=$($report.by_kind.expect_literal), panic_literal=$($report.by_kind.panic_literal)"
$md += ""
$md += "## Top Files"
$md += ""
$md += "| file | count |"
$md += "| ---- | ----: |"
foreach ($item in ($byFile | Select-Object -First 20)) {
  $md += "| $($item.file) | $($item.count) |"
}
$md += ""
$md += "## Findings"
$md += ""
$md += "| kind | file | line | literal | text |"
$md += "| ---- | ---- | ---: | ---- | ---- |"
foreach ($f in $findings) {
  $safeText = ([string]$f.text).Replace("|", "\|")
  $safeLiteral = ([string]$f.literal).Replace("|", "\|")
  $md += "| $($f.kind) | $($f.file) | $($f.line) | $safeLiteral | $safeText |"
}

Set-Content -LiteralPath $MarkdownOut -Value $md -Encoding UTF8

Write-Output "error_guard_json=$JsonOut"
Write-Output "error_guard_md=$MarkdownOut"
Write-Output "scanned_files=$($report.scanned_files)"
Write-Output "non_stable_literal_total=$($report.non_stable_literal_total)"
Write-Output "expect_literal=$($report.by_kind.expect_literal)"
Write-Output "panic_literal=$($report.by_kind.panic_literal)"
