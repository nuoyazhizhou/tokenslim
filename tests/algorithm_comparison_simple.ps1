# Simple Algorithm Comparison Test Script

Write-Host "=== TokenSlim Algorithm Comparison Test ===" -ForegroundColor Cyan
Write-Host ""

$testFiles = @(
    "tests\data\gcc_build_success.txt",
    "tests\data\gcc_coverity_success-1.txt"
)

$outputDir = "tests\output\algorithm_comparison"

# Create output directory
if (!(Test-Path $outputDir)) {
    New-Item -ItemType Directory -Path $outputDir -Force | Out-Null
}

# Test results
$results = @()

# Test 1: Current implementation
Write-Host "Test 1: Current Implementation" -ForegroundColor Yellow

foreach ($file in $testFiles) {
    $fileName = Split-Path $file -Leaf
    Write-Host "  Processing: $fileName"
    
    $origSize = (Get-Item $file).Length
    $outputFile = Join-Path $outputDir "${fileName}_current.json"
    
    # Execute compression
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $result = cargo run --release -- --compress --input $file --output $outputFile 2>&1
    $sw.Stop()
    
    $elapsed = $sw.ElapsedMilliseconds / 1000.0
    
    if ($LASTEXITCODE -eq 0) {
        $compSize = (Get-Item $outputFile).Length
        $ratio = [math]::Round($compSize / $origSize * 100, 2)
        
        Write-Host "    OK $($origSize) -> $($compSize) bytes (${ratio}%), {$($elapsed)}s"
        
        $results += @{
            Test = "Current Implementation"
            File = $fileName
            OriginalSize = $origSize
            CompressedSize = $compSize
            Ratio = $ratio
            Time = $elapsed
        }
    } else {
        Write-Host "    FAILED"
    }
}

# Test 2: Optimized path extraction
Write-Host "\nTest 2: Optimized Path Extraction" -ForegroundColor Yellow

# Backup original file
Copy-Item "src\core\path_analyzer\methods.rs" "src\core\path_analyzer\methods.rs.bak" -Force

# Replace with optimized version
Copy-Item "src\core\path_analyzer\optimized_methods.rs" "src\core\path_analyzer\methods.rs" -Force

# Build
cargo build --release

foreach ($file in $testFiles) {
    $fileName = Split-Path $file -Leaf
    Write-Host "  Processing: $fileName"
    
    $origSize = (Get-Item $file).Length
    $outputFile = Join-Path $outputDir "${fileName}_optimized_paths.json"
    
    # Execute compression
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $result = cargo run --release -- --compress --input $file --output $outputFile 2>&1
    $sw.Stop()
    
    $elapsed = $sw.ElapsedMilliseconds / 1000.0
    
    if ($LASTEXITCODE -eq 0) {
        $compSize = (Get-Item $outputFile).Length
        $ratio = [math]::Round($compSize / $origSize * 100, 2)
        
        Write-Host "    OK $($origSize) -> $($compSize) bytes (${ratio}%), {$($elapsed)}s"
        
        $results += @{
            Test = "Optimized Path Extraction"
            File = $fileName
            OriginalSize = $origSize
            CompressedSize = $compSize
            Ratio = $ratio
            Time = $elapsed
        }
    } else {
        Write-Host "    FAILED"
    }
}

# Restore original file
Copy-Item "src\core\path_analyzer\methods.rs.bak" "src\core\path_analyzer\methods.rs" -Force
Remove-Item "src\core\path_analyzer\methods.rs.bak" -Force

# Rebuild
cargo build --release

# Test 3: Enhanced macro handling
Write-Host "\nTest 3: Enhanced Macro Handling" -ForegroundColor Yellow

# Backup original file
Copy-Item "src\plugins\gcc_log_plugin\methods.rs" "src\plugins\gcc_log_plugin\methods.rs.bak" -Force

# Read original content
$originalContent = Get-Content "src\plugins\gcc_log_plugin\methods.rs" -Raw

# Create enhanced version
$enhancedContent = $originalContent -replace "fn replace_macros_in_line\(line: &str, dict_engine: &mut DictionaryEngine\) -> String \{[^\}]+\}", "fn replace_macros_in_line(line: &str, dict_engine: &mut DictionaryEngine) -> String {
    use std::cell::RefCell;
    thread_local! {
        // 匹配编译参数：-D, -I, -L, -std=, -O, -W, -f, -m, -Wall, -Wextra 等
        static MACRO_RE: RefCell<Regex> = RefCell::new(
            Regex::new(r"(-[DIULfWom][\\w\\.\\-\\+=]+|-std=\\w+|-O[0-9z]+|-Wall|-Wextra|-fPIC|-fomit-frame-pointer|-pipe)").unwrap()
        );
    }

    let mut result = line.to_string();
    MACRO_RE.with(|re| {
        let re = re.borrow();

        // 收集所有宏并按长度排序
        let mut macros: Vec<String> = re.find_iter(line).map(|m| m.as_str().to_string()).collect();
        macros.sort_by(|a, b| b.len().cmp(&a.len()));
        macros.dedup();

        // 优化：将相似的宏组合成一个token
        let mut macro_groups: Vec<Vec<String>> = Vec::new();
        for mac in &macros {
            let mut added = false;
            for group in &mut macro_groups {
                // 检查是否是相似的宏（如 -DXXX 系列）
                if mac.starts_with("-D") && group[0].starts_with("-D") {
                    group.push(mac.clone());
                    added = true;
                    break;
                }
            }
            if !added {
                macro_groups.push(vec![mac.clone()]);
            }
        }

        // 为每个组生成一个token
        for group in &macro_groups {
            if group.len() > 1 {
                // 为组生成一个token
                let group_token = dict_engine.add_macro(&group.join(" "));
                // 替换组中的所有宏
                for mac in group {
                    result = result.replace(mac.as_str(), &group_token);
                }
            } else if let Some(mac) = group.first() {
                let token = dict_engine.add_macro(mac);
                result = result.replace(mac.as_str(), &token);
            }
        }
    });
    result
}"

# Write enhanced version
Set-Content "src\plugins\gcc_log_plugin\methods.rs" $enhancedContent

# Build
cargo build --release

foreach ($file in $testFiles) {
    $fileName = Split-Path $file -Leaf
    Write-Host "  Processing: $fileName"
    
    $origSize = (Get-Item $file).Length
    $outputFile = Join-Path $outputDir "${fileName}_enhanced_macros.json"
    
    # Execute compression
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $result = cargo run --release -- --compress --input $file --output $outputFile 2>&1
    $sw.Stop()
    
    $elapsed = $sw.ElapsedMilliseconds / 1000.0
    
    if ($LASTEXITCODE -eq 0) {
        $compSize = (Get-Item $outputFile).Length
        $ratio = [math]::Round($compSize / $origSize * 100, 2)
        
        Write-Host "    OK $($origSize) -> $($compSize) bytes (${ratio}%), {$($elapsed)}s"
        
        $results += @{
            Test = "Enhanced Macro Handling"
            File = $fileName
            OriginalSize = $origSize
            CompressedSize = $compSize
            Ratio = $ratio
            Time = $elapsed
        }
    } else {
        Write-Host "    FAILED"
    }
}

# Restore original file
Copy-Item "src\plugins\gcc_log_plugin\methods.rs.bak" "src\plugins\gcc_log_plugin\methods.rs" -Force
Remove-Item "src\plugins\gcc_log_plugin\methods.rs.bak" -Force

# Rebuild
cargo build --release

# Generate report
Write-Host ""
Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "Algorithm Comparison Report" -ForegroundColor Cyan
Write-Host "================================================================================" -ForegroundColor Cyan

Write-Host ""
Write-Host "Detailed Results:" -ForegroundColor Yellow
Write-Host ("{0,-30} {1,-30} {2,10} {3,10} {4,8} {5,10}" -f "Test", "File", "Original", "Compressed", "Ratio", "Time")
Write-Host ("-" * 110)

foreach ($r in $results) {
    Write-Host ("{0,-30} {1,-30} {2,10} {3,10} {4,7}% {5,9}s" -f 
        $r.Test, 
        $r.File, 
        $r.OriginalSize, 
        $r.CompressedSize, 
        $r.Ratio, 
        [math]::Round($r.Time, 3))
}

# Calculate average performance per test
Write-Host ""
Write-Host "Average Performance by Test:" -ForegroundColor Yellow
Write-Host ("{0,-30} {1,10} {2,10} {3,10}" -f "Test", "Avg Ratio", "Avg Time", "Best?")
Write-Host ("-" * 70)

$tests = $results | Group-Object Test
foreach ($test in $tests) {
    $avgRatio = [math]::Round(($test.Group | Measure-Object Ratio -Average).Average, 2)
    $avgTime = [math]::Round(($test.Group | Measure-Object Time -Average).Average, 3)
    $best = ""
    
    # Determine if this is the best test
    $minRatio = ($tests | ForEach-Object { [math]::Round(($_.Group | Measure-Object Ratio -Average).Average, 2) } | Measure-Object -Minimum).Minimum
    if ($avgRatio -eq $minRatio) {
        $best = "✓"
    }
    
    Write-Host ("{0,-30} {1,9}% {2,9}s {3,6}" -f $test.Name, $avgRatio, $avgTime, $best)
}

Write-Host ""
Write-Host "Test completed!" -ForegroundColor Green
