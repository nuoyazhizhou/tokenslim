# 脱敏漏网之鱼全历史终审 (v2: 排除 Cargo.lock / .rs / .toml 误报)
$ErrorActionPreference = "SilentlyContinue"

Set-Location C:\git_work\TokenSlim-publish

# 只扫 samples/、tests/、*.scenario.yaml、*.json 这些"内容文件"
# 排除 Cargo.lock / 源码 / 配置
$patterns = @{
    "设备型号/产品名" = "WiiM|a98_|a97_|w98_|Firmware_build|Vibelink|Product\s+[A-Z]"
    "真实商家域名"   = "tweakers\.|galaxus\.|coolblue\.|mediamarkt|bol\.com|amazon\.de"
    "爬虫关键词"     = "blocked-vendor|test\.\s?de\b|test\.\s?nl\b"
}

$allCommits = git rev-list --all
Write-Host "[info] 全历史 commit 数: $($allCommits.Count)" -ForegroundColor Cyan
Write-Host "[info] 范围: samples/ + tests/ + scenario.yaml (排除 Cargo.lock / 源码)" -ForegroundColor Cyan

# 列出每个 commit 中命中了哪些 *文件*，并归并去重
$fileHits = @{}
foreach ($rev in $allCommits) {
    foreach ($name in $patterns.Keys) {
        $pat = $patterns[$name]
        $matched = git grep -lE $pat $rev -- 'samples/' 'tests/' '*.scenario.yaml' '*.json' 2>$null
        if ($matched) {
            foreach ($f in $matched) {
                $key = "$rev :: $f"
                $fileHits[$key] = $true
            }
        }
    }
}

Write-Host ""
Write-Host "=== 命中文件清单（去重 commit × 路径）===" -ForegroundColor Yellow
$fileHits.Keys | Sort-Object | ForEach-Object { Write-Host "  $_" }

Write-Host ""
if ($fileHits.Count -eq 0) {
    Write-Host "[GATE] PASS: 0 命中" -ForegroundColor Green
    exit 0
} else {
    Write-Host "[GATE] FAIL: $($fileHits.Count) 个 (commit, 路径) 组合仍含敏感词" -ForegroundColor Red
    exit 1
}
