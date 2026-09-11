# Algorithm Comparison Test Script

Write-Host "=== TokenSlim Algorithm Comparison Test ===" -ForegroundColor Cyan
Write-Host ""

$testFiles = @(
    "tests\data\gcc_build_success.txt",
    "tests\data\gcc_build_utf8.txt",
    "tests\data\gcc_coverity_success-1.txt",
    "tests\data\gcc_coverity_success-2.txt"
)

$outputDir = "tests\output\algorithm_comparison"

# Create output directory
if (!(Test-Path $outputDir)) {
    New-Item -ItemType Directory -Path $outputDir -Force | Out-Null
}

# Test results
$results = @()

# Test current implementation
Write-Host "Testing Current Implementation..." -ForegroundColor Yellow

foreach ($file in $testFiles) {
    $fileName = Split-Path $file -Leaf
    Write-Host "  Processing: $fileName" -NoNewline
    
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
        
        Write-Host "  OK $($origSize) -> $($compSize) bytes (${ratio}%), {$($elapsed)}s" -ForegroundColor Green
        
        $results += @{
            Algorithm = "Current Implementation"
            File = $fileName
            OriginalSize = $origSize
            CompressedSize = $compSize
            Ratio = $ratio
            Time = $elapsed
        }
    } else {
        Write-Host "  FAILED" -ForegroundColor Red
    }
}

# Test with optimized path extraction
Write-Host ""
Write-Host "Testing Optimized Path Extraction..." -ForegroundColor Yellow

# Create a modified version of path_analyzer for testing
$pathAnalyzerContent = Get-Content "src\core\path_analyzer\methods.rs" -Raw
$optimizedContent = $pathAnalyzerContent -replace "fn extract_common_paths\(.*?\)\s*\{.*?\}", "fn extract_common_paths(node: &TreeNode, current_path: &str, separator: &str, dict: &mut HashMap<String, String>, counter: &mut i32) {
    // 优化：只提取长度超过20的路径
    for (part, child) in &node.children {
        let new_path = if current_path.is_empty() {
            part.to_string()
        } else {
            format!("{}{}{}", current_path, separator, part)
        };
        
        // 优化：增加最小路径长度阈值
        if child.count > 1 && new_path.len() > 20 {
            let token = format!("$P{}", counter);
            dict.insert(new_path.clone(), token);
            *counter += 1;
        }
        
        // 递归处理子节点
        extract_common_paths(child, &new_path, separator, dict, counter);
    }
}"

# Write optimized version
Set-Content "src\core\path_analyzer\methods.rs" $optimizedContent

# Build and test
cargo build --release

foreach ($file in $testFiles) {
    $fileName = Split-Path $file -Leaf
    Write-Host "  Processing: $fileName" -NoNewline
    
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
        
        Write-Host "  OK $($origSize) -> $($compSize) bytes (${ratio}%), {$($elapsed)}s" -ForegroundColor Green
        
        $results += @{
            Algorithm = "Optimized Path Extraction"
            File = $fileName
            OriginalSize = $origSize
            CompressedSize = $compSize
            Ratio = $ratio
            Time = $elapsed
        }
    } else {
        Write-Host "  FAILED" -ForegroundColor Red
    }
}

# Test with enhanced macro handling
Write-Host ""
Write-Host "Testing Enhanced Macro Handling..." -ForegroundColor Yellow

# Restore original path analyzer
Set-Content "src\core\path_analyzer\methods.rs" $pathAnalyzerContent

# Modify gcc plugin for enhanced macro handling
$gccPluginContent = Get-Content "src\plugins\gcc_log_plugin\methods.rs" -Raw
$enhancedMacroContent = $gccPluginContent -replace "fn replace_macros_in_line\(.*?\)\s*\{.*?\}", "fn replace_macros_in_line(line: &str, dict_engine: &mut DictionaryEngine) -> String {
    use std::cell::RefCell;
    thread_local! {
        // 匹配编译参数：-D, -I, -L, -std=, -O, -W, -f, -m, -Wall, -Wextra 等
        static MACRO_RE: RefCell<Regex> = RefCell::new(
            Regex::new(r"(-[DIULfWom][\w\.\-\+=]+|-std=\w+|-O[0-9z]+|-Wall|-Wextra|-fPIC|-fomit-frame-pointer|-pipe)").unwrap()
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
Set-Content "src\plugins\gcc_log_plugin\methods.rs" $enhancedMacroContent

# Build and test
cargo build --release

foreach ($file in $testFiles) {
    $fileName = Split-Path $file -Leaf
    Write-Host "  Processing: $fileName" -NoNewline
    
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
        
        Write-Host "  OK $($origSize) -> $($compSize) bytes (${ratio}%), {$($elapsed)}s" -ForegroundColor Green
        
        $results += @{
            Algorithm = "Enhanced Macro Handling"
            File = $fileName
            OriginalSize = $origSize
            CompressedSize = $compSize
            Ratio = $ratio
            Time = $elapsed
        }
    } else {
        Write-Host "  FAILED" -ForegroundColor Red
    }
}

# Restore original files
Set-Content "src\plugins\gcc_log_plugin\methods.rs" $gccPluginContent
cargo build --release

# Generate report
Write-Host ""
Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "Algorithm Comparison Report" -ForegroundColor Cyan
Write-Host "================================================================================" -ForegroundColor Cyan

Write-Host ""
Write-Host "Detailed Results:" -ForegroundColor Yellow
Write-Host ("{0,-25} {1,-30} {2,10} {3,10} {4,8} {5,10}" -f "Algorithm", "File", "Original", "Compressed", "Ratio", "Time")
Write-Host ("-" * 100)

foreach ($r in $results) {
    Write-Host ("{0,-25} {1,-30} {2,10} {3,10} {4,7}% {5,9}s" -f 
        $r.Algorithm, 
        $r.File, 
        $r.OriginalSize, 
        $r.CompressedSize, 
        $r.Ratio, 
        [math]::Round($r.Time, 3))
}

# Calculate average performance per algorithm
Write-Host ""
Write-Host "Average Performance by Algorithm:" -ForegroundColor Yellow
Write-Host ("{0,-25} {1,10} {2,10} {3,10}" -f "Algorithm", "Avg Ratio", "Avg Time", "Best?")
Write-Host ("-" * 70)

$algorithms = $results | Group-Object Algorithm
foreach ($alg in $algorithms) {
    $avgRatio = [math]::Round(($alg.Group | Measure-Object Ratio -Average).Average, 2)
    $avgTime = [math]::Round(($alg.Group | Measure-Object Time -Average).Average, 3)
    $best = ""
    
    # Determine if this is the best algorithm
    $minRatio = ($algorithms | ForEach-Object { [math]::Round(($_.Group | Measure-Object Ratio -Average).Average, 2) } | Measure-Object -Minimum).Minimum
    if ($avgRatio -eq $minRatio) {
        $best = "✓"
    }
    
    Write-Host ("{0,-25} {1,9}% {2,9}s {3,6}" -f $alg.Name, $avgRatio, $avgTime, $best)
}

Write-Host ""
Write-Host "Test completed!" -ForegroundColor Green
