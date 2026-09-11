param(
    [int]$SizeMB = 100,
    [int]$Iterations = 3,
    [string]$SeedFile = "tests/data/gcc_build_failure-4.txt",
    [string]$InputFile = "benchmarks/input_100mb.txt",
    [string]$ReportFile = "docs/benchmark_report.md",
    [string]$JsonFile = "docs/benchmark_report.json"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Ensure-Dir([string]$path) {
    $dir = Split-Path -Parent $path
    if ($dir -and -not (Test-Path -LiteralPath $dir)) {
        New-Item -ItemType Directory -Path $dir | Out-Null
    }
}

Ensure-Dir $InputFile
Ensure-Dir $ReportFile
Ensure-Dir $JsonFile

$targetBytes = $SizeMB * 1024 * 1024
$regen = $true
if (Test-Path -LiteralPath $InputFile) {
    $existingSize = (Get-Item -LiteralPath $InputFile).Length
    if ($existingSize -eq $targetBytes) {
        $regen = $false
    }
}

if ($regen) {
    if (Test-Path -LiteralPath $SeedFile) {
        $seed = Get-Content -LiteralPath $SeedFile -Raw
    } else {
        $seed = "[2026-01-01T00:00:00Z] ERROR build failed in /jenkins/workspace/project/src/main.c`n"
    }
    if ([string]::IsNullOrWhiteSpace($seed)) {
        throw "Seed content is empty: $SeedFile"
    }

    $sb = New-Object System.Text.StringBuilder
    while ($sb.Length -lt $targetBytes) {
        [void]$sb.Append($seed)
    }
    $text = $sb.ToString().Substring(0, $targetBytes)
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText((Resolve-Path ".").Path + "\" + $InputFile, $text, $utf8NoBom)
    Write-Host "Generated benchmark input: $InputFile ($SizeMB MB)"
} else {
    Write-Host "Reuse benchmark input: $InputFile"
}

cargo run --release --bin pipeline_bench -- `
    --input $InputFile `
    --report $ReportFile `
    --json $JsonFile `
    --iterations $Iterations

Write-Host "Done. Report: $ReportFile"
