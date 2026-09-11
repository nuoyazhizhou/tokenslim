param(
    [Parameter(Mandatory=$true)]
    [string]$Message
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Continue"

$HistoryFile = "docs/benchmark_history.md"
$Dir = Split-Path -Parent $HistoryFile
if (-not (Test-Path $Dir)) {
    New-Item -ItemType Directory -Path $Dir | Out-Null
}

Write-Host "Running criterion benchmarks..."
# Run bench and capture output
$Output = cargo bench --bench core_benchmark 2>&1 | Out-String

# Filter out verbose cargo output
$FilteredOutput = $Output -split "`r?`n" | Where-Object { 
    $_ -notmatch '^\s*Compiling' -and 
    $_ -notmatch '^\s*Finished' -and 
    $_ -notmatch '^\s*Running' -and 
    $_ -notmatch '^\s*Downloaded' -and 
    $_ -notmatch '^\s*Blocking waiting' 
} | Out-String

$Date = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
$Commit = ""
try {
    $Commit = git rev-parse --short HEAD 2>$null
    if ([string]::IsNullOrWhiteSpace($Commit)) {
        $Commit = "unknown"
    }
} catch {
    $Commit = "unknown"
}

$Entry = "### [$Date] $Message (Commit: $Commit)`n`n"
$Entry += '```text' + "`n"
$Entry += $FilteredOutput.Trim() + "`n"
$Entry += '```' + "`n`n---`n"

if (Test-Path $HistoryFile) {
    $Current = Get-Content $HistoryFile -Raw
    $NewContent = $Current.TrimEnd() + "`n`n" + $Entry
    [IO.File]::WriteAllText((Resolve-Path $HistoryFile).Path, $NewContent, [Text.Encoding]::UTF8)
} else {
    $Header = "# Benchmark History`n`n记录历次性能优化的吞吐量与耗时变化。`n`n---`n`n"
    # Using absolute path for WriteAllText to avoid PS relative path issues
    $AbsPath = Join-Path (Get-Location).Path $HistoryFile
    [IO.File]::WriteAllText($AbsPath, $Header + $Entry, [Text.Encoding]::UTF8)
}

Write-Host "Benchmark recorded successfully to $HistoryFile"