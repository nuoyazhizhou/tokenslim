#!/usr/bin/env pwsh
# sync-from-main.ps1
#
# 对比主仓 (开发) 与发布仓 (脱敏后), 列出变化的文件, 让用户逐个确认是否同步。
# 同步完成后可选地 bump version + commit + tag + push 触发 CI 编译。
#
# 核心安全机制:
#   1. $protectedGlobs  : 强制保护, 永不覆盖 (即使用户选 A/M)
#   2. $userExcludeGlobs: 用户自定义排除 (脚本顶部维护)
#   3. 默认逐个确认:    每个 modified 文件都显示 diff 并要求 y/n
#
# Usage:
#   pwsh -NoProfile -File scripts/sync-from-main.ps1
#   pwsh -NoProfile -File scripts/sync-from-main.ps1 -AutoBump 0.2.7
#   pwsh -NoProfile -File scripts/sync-from-main.ps1 -DiffLines 50
#
# 退出码: 0=成功, 1=错误, 2=用户取消

[CmdletBinding()]
param(
    [string]$Src = "C:\git_work\TokenSlim",
    [string]$Dst = "C:\git_work\TokenSlim-publish2",
    [string]$AutoBump = "",
    [int]$DiffLines = 30,
    [switch]$ListOnly
)

$ErrorActionPreference = "Stop"

# ── 排除 (不参与 diff) ────────────────────────────────────────────────
$excludeDirs  = @('.git', 'dist', 'target', 'node_modules', '__pycache__',
                  '.gradle', '.vs', '.idea', '.pytest_cache', 'out', 'vendor',
                  'release-artifacts', 'tmp', 'scratch')
$excludeFiles = @('private-sweep.ps1', 'publish-rebuild-step1-copy.ps1',
                  'private-sweep-verify.ps1', 'tag-status.ps1',
                  'sync-from-main.ps1')   # 脚本自身不参与 diff

# ── 强制保护 (永不覆盖 publish2 已脱敏/特殊处理的文件) ────────────────
# 这些是已脱敏样本, 主仓版本可能含原始隐私信息, 绝不能让主仓版本反向覆盖
$protectedGlobs = @(
    'samples/cloud_log_plugin/case_04{5,6,7,8}*',
    'samples/cloud_log_plugin/case_04{5,6,7,8}.*'
)
# 同步匹配: 用 -like 通配符
$protectedPatterns = @(
    'samples/cloud_log_plugin/case_045*',
    'samples/cloud_log_plugin/case_046*',
    'samples/cloud_log_plugin/case_047*',
    'samples/cloud_log_plugin/case_048*'
)

# ── 用户自定义排除 (脚本顶部维护) ────────────────────────────────────
# 想长期排除某类文件时改这里
$userExcludePatterns = @(
    # 'tests/e2e_*',
    # 'samples/experimental/*',
)

# ── 发布仓"该有"的核心目录 ([A] 模式过滤 added 时用) ─────────────────
$corePrefixes = @(
    'src/',
    'tests/',
    'packages/',
    'config/',
    'Cargo.toml',
    'Cargo.lock',
    'README.md',
    'LICENSE'
)

# ── 校验 ──────────────────────────────────────────────────────────────
if (-not (Test-Path $Src)) {
    Write-Host "[ERR] 主仓不存在: $Src" -ForegroundColor Red
    Write-Host "      用 -Src <path> 指定主仓路径" -ForegroundColor Red
    exit 1
}
if (-not (Test-Path $Dst)) {
    Write-Host "[ERR] 发布仓不存在: $Dst" -ForegroundColor Red
    Write-Host "      用 -Dst <path> 指定发布仓路径" -ForegroundColor Red
    exit 1
}

# ── 收集文件 hash ─────────────────────────────────────────────────────
function Get-FileHashMap {
    param($Root, [string[]]$ExclDirs, [string[]]$ExclFiles)
    Get-ChildItem -Path $Root -Recurse -File -Force -ErrorAction SilentlyContinue |
        Where-Object {
            $rel = $_.FullName.Substring($Root.Length).TrimStart('\', '/').Replace('\', '/')
            $dirHit  = $ExclDirs  | Where-Object { $rel -like "$_/*" -or $rel -like "*/$_/*" }
            $fileHit = $ExclFiles | Where-Object { $_.Name -eq $_ }
            -not $dirHit -and -not $fileHit
        } |
        ForEach-Object {
            $rel  = $_.FullName.Substring($Root.Length).TrimStart('\', '/').Replace('\', '/')
            $hash = (Get-FileHash $_.FullName -Algorithm SHA256).Hash
            [PSCustomObject]@{
                RelPath  = $rel
                Hash     = $hash
                FullPath = $_.FullName
                Size     = $_.Length
            }
        }
}

function Test-Protected {
    param([string]$RelPath, [string[]]$Patterns)
    foreach ($p in $Patterns) {
        if ($RelPath -like $p) { return $true }
    }
    return $false
}

function Test-UserExcluded {
    param([string]$RelPath, [string[]]$Patterns)
    foreach ($p in $Patterns) {
        if ($RelPath -like $p) { return $true }
    }
    return $false
}

# ── helper: 显示 unified diff (前 N 行 src vs dst 并列) ───────────────
function Show-Diff {
    param($SrcPath, $DstPath, $RelPath, $MaxLines)
    $srcContent = Get-Content $SrcPath -Raw -ErrorAction SilentlyContinue
    $dstContent = Get-Content $DstPath -Raw -ErrorAction SilentlyContinue

    Write-Host ""
    Write-Host ("  --- diff ({0}) ---" -f $RelPath) -ForegroundColor Magenta

    if (-not $srcContent) { Write-Host "  [skip] src 不可读" -ForegroundColor Red; return }
    if (-not $dstContent) { Write-Host "  [info] dst 不存在 (new file)" -ForegroundColor Yellow }

    $srcLines = $srcContent -split "`n"
    $dstLines = $dstContent -split "`n"

    $showCount = [Math]::Min($MaxLines, [Math]::Max($srcLines.Length, $dstLines.Length))
    Write-Host "  src 长度: $($srcLines.Length) 行" -ForegroundColor DarkGray
    Write-Host "  dst 长度: $($dstLines.Length) 行" -ForegroundColor DarkGray
    Write-Host ""
    Write-Host "  --- src 前 $showCount 行 (主仓) ---" -ForegroundColor Cyan
    $srcLines | Select-Object -First $showCount | ForEach-Object {
        $line = if ($_.Length -gt 120) { $_.Substring(0, 120) + "..." } else { $_ }
        Write-Host "    $line" -ForegroundColor White
    }
    Write-Host ""
    Write-Host "  --- dst 前 $showCount 行 (发布仓/已脱敏) ---" -ForegroundColor Cyan
    $dstLines | Select-Object -First $showCount | ForEach-Object {
        $line = if ($_.Length -gt 120) { $_.Substring(0, 120) + "..." } else { $_ }
        Write-Host "    $line" -ForegroundColor DarkGray
    }
    Write-Host "  --- end ---" -ForegroundColor Magenta
}

# ── 扫描 ──────────────────────────────────────────────────────────────
Write-Host "[scan] 计算文件 hash ..." -ForegroundColor Cyan
Write-Host "  src: $Src"
$srcFiles = @(Get-FileHashMap $Src $excludeDirs $excludeFiles)
Write-Host "       $($srcFiles.Count) 个文件" -ForegroundColor Green
Write-Host "  dst: $Dst"
$dstFiles = @(Get-FileHashMap $Dst $excludeDirs $excludeFiles)
Write-Host "       $($dstFiles.Count) 个文件" -ForegroundColor Green

# ── diff ──────────────────────────────────────────────────────────────
$srcMap = @{}; foreach ($f in $srcFiles) { $srcMap[$f.RelPath] = $f }
$dstMap = @{}; foreach ($f in $dstFiles) { $dstMap[$f.RelPath] = $f }

$rawAdded = @(); $rawModified = @(); $deleted = @(); $unchanged = 0
foreach ($p in $srcMap.Keys) {
    if (-not $dstMap.ContainsKey($p)) { $rawAdded += $srcMap[$p] }
    elseif ($dstMap[$p].Hash -ne $srcMap[$p].Hash) { $rawModified += $srcMap[$p] }
    else { $unchanged++ }
}
foreach ($p in $dstMap.Keys) {
    if (-not $srcMap.ContainsKey($p)) { $deleted += $dstMap[$p] }
}

# 应用过滤器
$protectedHits = @()
$userExcludedHits = @()

$added = @()
foreach ($f in $rawAdded) {
    if (Test-Protected $f.RelPath $protectedPatterns) { $protectedHits += $f; continue }
    if (Test-UserExcluded $f.RelPath $userExcludePatterns) { $userExcludedHits += $f; continue }
    $added += $f
}
$modified = @()
foreach ($f in $rawModified) {
    if (Test-Protected $f.RelPath $protectedPatterns) { $protectedHits += $f; continue }
    if (Test-UserExcluded $f.RelPath $userExcludePatterns) { $userExcludedHits += $f; continue }
    $modified += $f
}

# ── 显示汇总 ──────────────────────────────────────────────────────────
Write-Host ""
Write-Host "=== 差异汇总 ===" -ForegroundColor Cyan
Write-Host ("  raw added     : {0}  (主仓有 / 发布仓没有)" -f $rawAdded.Count)
Write-Host ("  raw modified  : {0}  (两边都有, hash 不同)" -f $rawModified.Count)
Write-Host ("  raw deleted   : {0}  (主仓删了 / 发布仓还有)" -f $deleted.Count)
Write-Host ("  unchanged     : {0}" -f $unchanged)
Write-Host ""
Write-Host ("  filtered  added     : {0}" -f $added.Count)
Write-Host ("  filtered  modified  : {0}" -f $modified.Count)
Write-Host ("  filtered  deleted   : {0}" -f $deleted.Count)
if ($protectedHits.Count -gt 0) {
    Write-Host ("  PROTECTED         : {0}  (强制保护, 永不覆盖)" -f $protectedHits.Count) -ForegroundColor Red
}
if ($userExcludedHits.Count -gt 0) {
    Write-Host ("  user-excluded     : {0}" -f $userExcludedHits.Count) -ForegroundColor DarkYellow
}

# ── 强制保护命中提示 ──────────────────────────────────────────────────
if ($protectedHits.Count -gt 0) {
    Write-Host ""
    Write-Host "=== [PROTECTED] 强制保护文件 (永不覆盖) ===" -ForegroundColor Red
    $protectedHits | ForEach-Object { Write-Host "  [SKIP] $($_.RelPath)" -ForegroundColor Red }
    Write-Host ""
    Write-Host "原因: 这些是 publish2 手动脱敏过的文件, 主仓版本可能含原始隐私信息" -ForegroundColor Red
    Write-Host "      让主仓版本反向覆盖会泄露隐私!" -ForegroundColor Red
    Write-Host "      如需重新脱敏, 请手动编辑 publish2 端, 不要走 sync。" -ForegroundColor Red
}

# ── 详细差异列表 ──────────────────────────────────────────────────────
if ($modified.Count -gt 0) {
    Write-Host ""
    Write-Host "=== modified 文件列表 (按目录分组) ===" -ForegroundColor Cyan
    $byDir = $modified | Group-Object { Split-Path $_.RelPath -Parent } | Sort-Object Count -Descending
    foreach ($g in $byDir) {
        Write-Host ("  [{0,3}] {1}" -f $g.Count, $g.Name) -ForegroundColor Yellow
        $g.Group | ForEach-Object {
            $dstPath = Join-Path $Dst $_.RelPath
            $dstHash = if (Test-Path $dstPath) { (Get-FileHash $dstPath -Algorithm SHA256).Hash.Substring(0,8) } else { "(none)" }
            Write-Host ("        {0,-60}  src={1}  dst={2}" -f $_.RelPath, $_.Hash.Substring(0,8), $dstHash) -ForegroundColor Gray
        }
    }
}

# 默认隐藏 added/deleted 详情, 用 -Verbose 打开
$showExtra = $PSBoundParameters.ContainsKey('Verbose') -or ($args -contains "-Verbose")
if ($showExtra) {
    if ($rawAdded.Count -gt 0) {
        Write-Host ""
        Write-Host "=== raw added (主仓新增) ===" -ForegroundColor Cyan
        $rawAdded | Select-Object -First 50 | ForEach-Object { Write-Host "  + $($_.RelPath)" }
        if ($rawAdded.Count -gt 50) { Write-Host "  ... (+$($rawAdded.Count - 50) more)" }
    }
    if ($deleted.Count -gt 0) {
        Write-Host ""
        Write-Host "=== deleted (主仓删了, 发布仓还有) ===" -ForegroundColor Cyan
        $deleted | ForEach-Object { Write-Host "  - $($_.RelPath)" }
    }
} else {
    Write-Host ""
    Write-Host "(用 -Verbose 查看 raw added / deleted 详情)" -ForegroundColor DarkGray
}

if ($userExcludedHits.Count -gt 0) {
    Write-Host ""
    Write-Host "=== user-excluded (脚本顶部 $userExcludePatterns 排除) ===" -ForegroundColor DarkYellow
    $userExcludedHits | ForEach-Object { Write-Host "  [X] $($_.RelPath)" -ForegroundColor DarkYellow }
}

# ── ListOnly 模式: 只列出 diff 不交互 (在菜单前, 避免 Read-Host 阻塞) ──
if ($ListOnly) {
    Write-Host ""
    Write-Host "=== [ListOnly 模式] 显示每个 modified 文件的前 $DiffLines 行 ===" -ForegroundColor Cyan
    foreach ($f in $modified) {
        $dstPath = Join-Path $Dst $f.RelPath
        Show-Diff $f.FullPath $dstPath $f.RelPath $DiffLines
    }
    Write-Host ""
    Write-Host "[done] ListOnly 模式, 不执行任何同步" -ForegroundColor Yellow
    exit 0
}

# ── 决策模式 ──────────────────────────────────────────────────────────
Write-Host ""
Write-Host "=== 决策模式 ===" -ForegroundColor Cyan
Write-Host "  [P] 逐个确认 (默认, 最安全) - 显示每个文件的 diff 后 y/n 决策"
Write-Host "  [A] 全部 modified (已过滤保护名单)"
Write-Host "  [D] 按目录批量 (逗号分隔)"
Write-Host "  [N] 不同步, 只查看"
Write-Host "  [Q] 退出"

$choice = ""
if ($AutoBump -or $args[0] -eq "--non-interactive") {
    $choice = "A"
    Write-Host "  (non-interactive: 自动选 A)" -ForegroundColor DarkGray
} else {
    $choice = (Read-Host "选择 [P/A/D/N/Q]").Trim().ToUpper()
    if (-not $choice) { $choice = "P" }
}

$toSync = @()

# ── 决策模式 ──────────────────────────────────────────────────────────
switch ($choice) {
    'N' { Write-Host "[skip] 不同步" -ForegroundColor Yellow; exit 0 }
    'Q' { Write-Host "[quit] 退出" -ForegroundColor Yellow; exit 2 }

    'P' {
        # 逐个确认
        if ($modified.Count -eq 0) {
            Write-Host "[info] 没有 modified 文件需要确认" -ForegroundColor Yellow
            exit 0
        }
        Write-Host ""
        Write-Host "=== 逐个确认 modified 文件 (共 $($modified.Count) 个) ===" -ForegroundColor Cyan
        $i = 0
        foreach ($f in $modified) {
            $i++
            $dstPath = Join-Path $Dst $f.RelPath
            Write-Host ""
            Write-Host ("  [{0}/{1}] {2}" -f $i, $modified.Count, $f.RelPath) -ForegroundColor Yellow
            Show-Diff $f.FullPath $dstPath $f.RelPath $DiffLines
            $ans = (Read-Host "  同步? [y/n/q=停止]").Trim().ToLower()
            switch ($ans) {
                'y' { $toSync += $f; Write-Host "    -> 已加入" -ForegroundColor Green }
                'q' { Write-Host "    -> 停止后续" -ForegroundColor Yellow; break }
                default { Write-Host "    -> 跳过" -ForegroundColor DarkGray }
            }
            if ($ans -eq 'q') { break }
        }
    }

    'A' {
        Write-Host "[A] mode: 全部 $($modified.Count) modified (保护名单已过滤)" -ForegroundColor Cyan
        $toSync = @($modified)
    }

    'D' {
        $input = Read-Host "输入目录前缀 (逗号分隔, 如 src/core, packages)"
        $prefixes = $input.Split(',') | ForEach-Object { $_.Trim() } | Where-Object { $_ }
        $toSync = @($modified) | Where-Object {
            $rel = $_.RelPath
            $matched = $false
            foreach ($pre in $prefixes) {
                if ($rel.StartsWith($pre)) { $matched = $true; break }
            }
            $matched
        }
    }

    default { Write-Host "[ERR] 无效选择" -ForegroundColor Red; exit 1 }
}

if ($toSync.Count -eq 0) {
    Write-Host "[info] 选中 0 个文件, 无需同步" -ForegroundColor Yellow
    exit 0
}

# ── 最终确认 ─────────────────────────────────────────────────────────
Write-Host ""
Write-Host "=== 最终待同步 $($toSync.Count) 个文件 ===" -ForegroundColor Cyan
$toSync | ForEach-Object { Write-Host "  $($_.RelPath)" }
Write-Host ""
$confirm = ""
if ($AutoBump -or $args[0] -eq "--non-interactive") {
    $confirm = "y"
} else {
    $confirm = (Read-Host "确认同步? [y/N]").Trim().ToLower()
}
if ($confirm -ne 'y') {
    Write-Host "[cancel] 用户取消" -ForegroundColor Yellow
    exit 2
}

# ── 执行同步 ─────────────────────────────────────────────────────────
$okCount = 0; $failCount = 0
foreach ($f in $toSync) {
    $targetPath = Join-Path $Dst $f.RelPath
    $targetDir  = Split-Path $targetPath -Parent
    if (-not (Test-Path $targetDir)) {
        New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
    }
    try {
        Copy-Item $f.FullPath $targetPath -Force
        $okCount++
    } catch {
        Write-Host ("  [FAIL] {0}: {1}" -f $f.RelPath, $_) -ForegroundColor Red
        $failCount++
    }
}

Write-Host ""
Write-Host ("[done] 同步成功 {0} 个, 失败 {1} 个" -f $okCount, $failCount) -ForegroundColor Green

# ── 跑脱敏扫 ─────────────────────────────────────────────────────────
$sweepScript = Join-Path $Dst "scripts\private-sweep-verify.ps1"
if (Test-Path $sweepScript) {
    Write-Host ""
    Write-Host "=== 跑脱敏扫 ===" -ForegroundColor Cyan
    & $sweepScript
} else {
    Write-Host "[warn] 脱敏扫脚本不存在: $sweepScript" -ForegroundColor Yellow
}

# ── 可选 bump + commit + tag + push ──────────────────────────────────
$newVer = $AutoBump
if (-not $newVer) {
    $input = Read-Host "`n新版本号 (留空跳过, 格式 0.2.7)"
    $newVer = $input.Trim()
}

if ($newVer -match '^\d+\.\d+\.\d+$') {
    Write-Host ""
    Write-Host "[bump] -> $newVer" -ForegroundColor Cyan
    Push-Location $Dst
    try {
        node scripts/bump-version.mjs $newVer
        if ($LASTEXITCODE -ne 0) { throw "bump-version.mjs 失败" }

        git add -A
        $commitMsg = "chore(release): sync from main and bump to $newVer"
        git commit -m $commitMsg
        if ($LASTEXITCODE -ne 0) { throw "git commit 失败" }

        git tag "v$newVer"
        Write-Host ""
        Write-Host "=== 准备 push ===" -ForegroundColor Cyan
        $pushConfirm = (Read-Host "push main + tag v$newVer 到 origin? [y/N]").Trim().ToLower()
        if ($pushConfirm -eq 'y') {
            git push origin main
            git push origin "v$newVer"
            Write-Host "[OK] push 完成, CI 将自动编译" -ForegroundColor Green
        } else {
            Write-Host "[skip] push 已跳过, 你可以手动: git push origin main && git push origin v$newVer" -ForegroundColor Yellow
        }
    } finally {
        Pop-Location
    }
} elseif ($newVer) {
    Write-Host "[ERR] 版本号格式错: '$newVer' (期望 0.2.7)" -ForegroundColor Red
    exit 1
} else {
    Write-Host "[skip] 不 bump version" -ForegroundColor Yellow
}
