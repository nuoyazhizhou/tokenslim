param(
  [string]$ReportPath = "target/vcs_compact_showcase_report.txt",
  [string]$TasksPath = "VCS_TASKS.md",
  [string]$CsvOut = "docs/vcs_compact_case_ratios_latest.csv",
  [string]$MdOut = "docs/vcs_compact_case_ratios_latest.md"
)

$reportFull = Resolve-Path $ReportPath -ErrorAction Stop
$tasksFull = Resolve-Path $TasksPath -ErrorAction Stop

$ratioMap = @{}
$currentCase = $null
foreach ($line in Get-Content -Path $reportFull -Encoding UTF8) {
  if ($line -match '^Case\s+(case_\d+)\s+-') {
    $currentCase = $matches[1]
    continue
  }
  if ($currentCase -and $line -match 'Compression:\s*(-?\d+(?:\.\d+)?)%') {
    $ratioMap[$currentCase] = [double]$matches[1]
    $currentCase = $null
  }
}

if ($ratioMap.Count -eq 0) {
  throw "No case ratios parsed from report: $ReportPath"
}

$lines = Get-Content -Path $tasksFull -Encoding UTF8
$out = New-Object System.Collections.Generic.List[string]

for ($i = 0; $i -lt $lines.Count; $i++) {
  $line = $lines[$i]

  if ($line -match '^> 更新时间：') {
    $out.Add('> 更新时间：2026-04-16')
    continue
  }

  if ($line -match '^\|\s*#\s*\|' -and $line -match '测试用例') {
    if ($line -notmatch '压缩比') {
      $line = ($line.TrimEnd() + ' 压缩比 |')
    }
    $out.Add($line)
    continue
  }

  if ($line -match '^\|\s*-+' -and $line -match '\|') {
    if ($i -gt 0 -and $lines[$i-1] -match '^\|\s*#\s*\|' -and $lines[$i-1] -match '测试用例') {
      if ($line -notmatch '\|\s*-+\s*\|\s*$') {
        $line = ($line.TrimEnd() + ' ------- |')
      }
    }
    $out.Add($line)
    continue
  }

  if ($line -match '^\|' -and $line -match 'case_\d+') {
    $caseIds = [regex]::Matches($line, 'case_\d+') | ForEach-Object { $_.Value }
    if ($caseIds.Count -gt 0) {
      $vals = @()
      foreach ($cid in $caseIds) {
        if ($ratioMap.ContainsKey($cid)) {
          $vals += ('{0:N1}%' -f $ratioMap[$cid])
        } else {
          $vals += 'N/A'
        }
      }
      $ratioText = ($vals -join ', ')

      $trimmed = $line.TrimEnd()
      if ($trimmed -match '\|\s*$') {
        # detect existing last col count by header approach not reliable; replace if already has ratio-looking trailing col
        $parts = $trimmed.Split('|')
        # markdown table line: leading+trailing empty segments
        if ($parts.Length -ge 7) {
          $parts[$parts.Length - 2] = ' ' + $ratioText + ' '
          $line = ($parts -join '|')
        } else {
          $line = ($trimmed + ' ' + $ratioText + ' |')
        }
      }
    }
  }

  $out.Add($line)
}

Set-Content -Path $tasksFull -Encoding UTF8 -Value $out

# Build persistent case ratio CSV + markdown from table rows in tasks
$updatedLines = Get-Content -Path $tasksFull -Encoding UTF8
$records = New-Object System.Collections.Generic.List[object]
foreach ($l in $updatedLines) {
  if ($l -match '^\|\s*\d+\.\d+\s*\|') {
    $cells = $l.Split('|') | ForEach-Object { $_.Trim() }
    if ($cells.Count -ge 6) {
      $idx = $cells[1]
      $cmd = $cells[2]
      $parser = $cells[3]
      $cases = $cells[4]
      $ratio = $cells[5]
      $records.Add([PSCustomObject]@{
        Index = $idx
        Command = $cmd
        Parser = $parser
        Cases = $cases
        Ratio = $ratio
      })
    }
  }
}

$csvDir = Split-Path -Parent $CsvOut
if ($csvDir -and -not (Test-Path $csvDir)) { New-Item -ItemType Directory -Path $csvDir | Out-Null }
$mdDir = Split-Path -Parent $MdOut
if ($mdDir -and -not (Test-Path $mdDir)) { New-Item -ItemType Directory -Path $mdDir | Out-Null }

$records | Export-Csv -Path $CsvOut -Encoding UTF8 -NoTypeInformation

$md = New-Object System.Collections.Generic.List[string]
$md.Add('# VCS Compact Case Ratios (Latest)')
$md.Add('')
$md.Add('更新日期: 2026-04-16')
$md.Add('来源: `cargo test vcs_ai_compact_showcase_all_cases -- --nocapture` 生成的 `target/vcs_compact_showcase_report.txt`')
$md.Add('')
$md.Add('| # | 命令 | Parser | 测试用例 | 压缩比 |')
$md.Add('| --- | --- | --- | --- | --- |')
foreach ($r in $records) {
  $md.Add("| $($r.Index) | $($r.Command) | $($r.Parser) | $($r.Cases) | $($r.Ratio) |")
}
Set-Content -Path $MdOut -Encoding UTF8 -Value $md

Write-Output "ratios_parsed=$($ratioMap.Count)"
Write-Output "rows_exported=$($records.Count)"
Write-Output "updated_tasks=$TasksPath"
Write-Output "csv=$CsvOut"
Write-Output "md=$MdOut"
