# Batch Compression Test Script

Write-Host "=== TokenSlim Batch Compression Test ===" -ForegroundColor Cyan
Write-Host ""

$dataDir = "tests\data"
$outputDir = "tests\output"

# Create output directory
if (!(Test-Path $outputDir)) {
    New-Item -ItemType Directory -Path $outputDir | Out-Null
}

# Get all test files
$files = Get-ChildItem -Path $dataDir -Filter *.txt | Sort-Object Name

Write-Host "Found $($files.Count) test files" -ForegroundColor Green
Write-Host ""

$totalOrigSize = 0
$totalCompSize = 0
$totalTime = 0
$results = @()

foreach ($file in $files) {
    Write-Host "Processing: $($file.Name)" -NoNewline
    
    $origSize = $file.Length
    $totalOrigSize += $origSize
    
    $outputFile = Join-Path $outputDir "$($file.Name).json"
    
    # Execute compression
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $result = cargo run --release -- --compress --input $file.FullName --output $outputFile 2>&1
    $sw.Stop()
    
    $elapsed = $sw.ElapsedMilliseconds / 1000.0
    $totalTime += $elapsed
    
    if ($LASTEXITCODE -eq 0) {
        $compSize = (Get-Item $outputFile).Length
        $totalCompSize += $compSize
        
        $ratio = [math]::Round($compSize / $origSize * 100, 2)
        
        Write-Host "  OK $($origSize) -> $($compSize) bytes (${$ratio}%), {$($elapsed)}s" -ForegroundColor Green
        
        $results += @{
            File = $file.Name
            OriginalSize = $origSize
            CompressedSize = $compSize
            Ratio = $ratio
            Time = $elapsed
        }
    } else {
        Write-Host "  FAILED" -ForegroundColor Red
        $results += @{
            File = $file.Name
            OriginalSize = $origSize
            CompressedSize = 0
            Ratio = 0
            Time = $elapsed
            Error = $true
        }
    }
}

Write-Host ""
Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "Batch Test Report" -ForegroundColor Cyan
Write-Host "================================================================================" -ForegroundColor Cyan

Write-Host ""
Write-Host "Overall Statistics:" -ForegroundColor Yellow
Write-Host "  Total files: $($files.Count)"
Write-Host "  Success: $($results.Where({$_.Error -ne $true}).Count)"
Write-Host "  Failed: $($results.Where({$_.Error -eq $true}).Count)"

if ($totalOrigSize -gt 0) {
    $overallRatio = [math]::Round($totalCompSize / $totalOrigSize * 100, 2)
    Write-Host ""
    Write-Host "Size Statistics:" -ForegroundColor Yellow
    Write-Host "  Original total size: $($totalOrigSize) bytes ($([math]::Round($totalOrigSize / 1MB, 2)) MB)"
    Write-Host "  Compressed total size: $($totalCompSize) bytes ($([math]::Round($totalCompSize / 1MB, 2)) MB)"
    Write-Host "  Average compression ratio: $($overallRatio)%"
    Write-Host "  Space saved: $($totalOrigSize - $totalCompSize) bytes ($([math]::Round(($totalOrigSize - $totalCompSize) / 1MB, 2)) MB)"
}

Write-Host ""
Write-Host "Performance Statistics:" -ForegroundColor Yellow
Write-Host "  Total processing time: $([math]::Round($totalTime, 3)) seconds"
if ($totalOrigSize -gt 0) {
    $throughput = [math]::Round($totalOrigSize / $totalTime / 1MB, 2)
    Write-Host "  Average throughput: $($throughput) MB/s"
}

Write-Host ""
Write-Host "Detailed Results:" -ForegroundColor Yellow
Write-Host ("{0,-40} {1,10} {2,10} {3,8} {4,10}" -f "File", "Original", "Compressed", "Ratio", "Time")
Write-Host ("-" * 80)

foreach ($r in $results) {
    $status = if ($r.Error) { "X" } else { "OK" }
    Write-Host ("{0,-40} {1,10} {2,10} {3,7}% {4,9}s {5}" -f 
        $r.File, 
        $r.OriginalSize, 
        $r.CompressedSize, 
        $r.Ratio, 
        [math]::Round($r.Time, 3),
        $status)
}

Write-Host ""
Write-Host "Test completed!" -ForegroundColor Green
