#!/usr/bin/env pwsh
<#
.SYNOPSIS
  把主仓 (TokenSlim) 选定的文件同步到发布仓 (TokenSlim-publish2), 默认逐个确认。

.DESCRIPTION
  在主仓下运行。对比两侧文件 hash, 列出 added/modified/deleted, 让用户
  选文件复制。强制保护 publish2 已脱敏的 case 文件, 同步后自动跑脱敏扫。

.PARAMETER Src
  主仓根路径。默认 = 脚本所在目录的父级 (= TokenSlim 根)。

.PARAMETER Dst
  发布仓根路径。默认 = C:\git_work\TokenSlim-publish2。

.PARAMETER DiffLines
  diff 预览显示前 N 行。默认 30。

.PARAMETER ListOnly
  只列出 diff 不交互, 不执行任何同步 (在决策模式菜单之前就退出, 不阻塞 Read-Host)。

.PARAMETER ShowNextSteps
  同步完成后是否打印 publish2 端的下一步命令清单。默认开。

.EXAMPLE
  pwsh -File scripts/sync-to-publish2.ps1 -ListOnly
  # 只看 diff, 不动手

.EXAMPLE
  pwsh -File scripts/sync-to-publish2.ps1
  # 交互式, 默认 P 模式逐个确认

.EXAMPLE
  pwsh -File scripts/sync-to-publish2.ps1 -DiffLines 50
  # diff 预览显示前 50 行

.EXAMPLE
  pwsh -File scripts/sync-to-publish2.ps1 -Src D:\work\TokenSlim -Dst D:\work\TokenSlim-publish2
  # 自定义路径

.NOTES
  Author : TokenSlim team
  退出码: 0=成功, 1=错误, 2=用户取消
  完整工作流:
    1. 主仓改代码
    2. pwsh -File scripts/sync-to-publish2.ps1  (或 -ListOnly 先看)
    3. 选文件 (P/A/D/N/Q)
    4. 同步完, 跳到 publish2 端跑:
         cd C:\git_work\TokenSlim-publish2
         node scripts/bump-version.mjs 0.2.6
         git add -A
         git commit -m "chore(release): v0.2.6"
         git tag v0.2.6
         git push origin main --tags
#>
# ── 在 PowerShell 中查看完整帮助:  Get-Help .\scripts\sync-to-publish2.ps1 -Full ──

[CmdletBinding()]
param(
    [string]$Src = (Split-Path $PSScriptRoot -Parent),
    [string]$Dst = "C:\git_work\TokenSlim-publish2",
    [int]$DiffLines = 30,
    [switch]$ListOnly,
    [switch]$ShowNextSteps = $true,
    [switch]$AutoA,
    [switch]$IncludeGitignore,
    [switch]$IncludeLock,
    [switch]$IncludeManifest,
    [switch]$Help
)

$ErrorActionPreference = "Stop"

# ── Help 开关: 直接打印, 不跑逻辑 ─────────────────────────────────────
if ($Help) {
    $helpText = @"
SYNOPSIS
  sync-to-publish2.ps1 - 把主仓 (TokenSlim) 选定的文件同步到发布仓 (TokenSlim-publish2)

USAGE
  pwsh -File scripts/sync-to-publish2.ps1 [options]

OPTIONS
  -Src <path>        主仓根路径 (默认 = 脚本所在目录的父级 = TokenSlim 根)
  -Dst <path>        发布仓根路径 (默认 = C:\git_work\TokenSlim-publish2)
  -DiffLines <int>   diff 预览显示前 N 行 (默认 30)
  -ListOnly          只列出 diff, 不交互, 不执行任何同步
  -ShowNextSteps     同步完成后是否打印 publish2 端的下一步命令 (默认开)
  -AutoA             非交互模式: 自动选 A 同步所有 modified, 跳过确认 (CI/脚本用)
  -IncludeGitignore  显式把 .gitignore 纳入同步 (默认排除, 主仓 ≠ 发布策略)
  -IncludeLock       显式把 Cargo.lock / package-lock.json / pnpm-lock.yaml 纳入同步 (默认排除, 双端 CRLF/LF 不一致)
  -IncludeManifest   显式把 Cargo.toml / package.json 纳入同步 (默认排除, 由 publish2 端 bump-version 改)
  -Help / -?         显示本帮助

EXAMPLES
  # 1. 先看 diff, 不动手
  pwsh -File scripts/sync-to-publish2.ps1 -ListOnly

  # 2. 交互式同步 (默认 P 模式逐个确认)
  pwsh -File scripts/sync-to-publish2.ps1

  # 3. diff 详细显示前 50 行
  pwsh -File scripts/sync-to-publish2.ps1 -DiffLines 50

  # 4. 自定义路径
  pwsh -File scripts/sync-to-publish2.ps1 -Src D:\work\TokenSlim -Dst D:\work\TokenSlim-publish2

完整工作流
  1. 主仓下改完代码
  2. pwsh -File scripts/sync-to-publish2.ps1 [-ListOnly 先看]
  3. 选文件 (P / A / D / N / Q)
  4. 同步完成 + 脱敏扫通过后, 跳到 publish2 端跑:
       cd C:\git_work\TokenSlim-publish2
       node scripts/bump-version.mjs 0.2.6
       git add -A
       git commit -m "chore(release): v0.2.6"
       git tag v0.2.6
       git push origin main --tags

退出码: 0=成功 / 1=错误 / 2=用户取消
"@
    Write-Host $helpText
    exit 0
}

# ── 排除 (不参与 diff) ────────────────────────────────────────────────
$excludeDirs  = @('.git', 'dist', 'target', 'node_modules', '__pycache__',
                  '.gradle', '.vs', '.idea', '.pytest_cache', 'out', 'vendor',
                  'release-artifacts', 'tmp', 'scratch')
$excludeFiles = @('private-sweep.ps1', 'publish-rebuild-step1-copy.ps1',
                  'private-sweep-verify.ps1', 'tag-status.ps1',
                  'sync-to-publish2.ps1', 'sync-from-main.ps1')

# 运行时排除的 dotfile 模式 (主仓根的同步备忘等, 不参与 diff 也不同步)
$runtimeExcludeGlobs = @('.sync-*')

# ── 默认不参与同步 (两边各自管, 主仓版本反向覆盖会破坏发布策略) ────────
# 这些文件在主仓和 publish2 端有不同的内容/换行符/编码, 一刀切同步会
#   - 覆盖 publish2 端精心维护的发布策略 (.gitignore)
#   - 引入 CRLF↔LF 噪声 (Cargo.lock / *.lock)
#   - 把开发锁换成发布锁, 破坏 CI 确定性
# 如果确实要同步某项, 用 -IncludeXxx 开关显式打开
$defaultExcludePatterns = @(
    # 仓库级: 各自管
    '.gitignore',                    # 各自管: 主仓 = 开发策略, publish2 = 发布策略
    'Cargo.lock',                    # 各自管: 锁文件 CRLF/LF 双端不一致
    'package-lock.json',            # 同 Cargo.lock
    'pnpm-lock.yaml',
    'yarn.lock',
    'Cargo.toml',                    # publish2 端 bump-version.mjs 改
    'package.json',                  # 同 Cargo.toml
    'CHANGELOG.md',                  # 各自写
    '.cargo/config.toml',            # 本地 cargo 配置
    '.cargo/config',
    # 已知开发期产物 (两边都应被 .gitignore 忽略, 不参与同步)
    'docs/audit/',                   # audit 中间快照
    'release-artifacts/',            # 发布产物
    'tokenslim-workspace/',          # tokenslim workspace 注入目录
    'tmp/',
    'scratch/',
    'dist/',
    'build/',
    'out/'
)

# ── 强制保护 (永不覆盖 publish2 已脱敏/特殊处理的文件) ────────────────
# 主仓版本可能含原始隐私信息, 绝不能让主仓版本反向覆盖
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
    param(
        $Root,
        [string[]]$ExclDirs,
        [string[]]$ExclFiles,
        [string[]]$ExclGlobs,
        [string[]]$DefExcl,
        [string[]]$DefDirs,
        [string[]]$GitignoreFiles,
        [string[]]$GitignoreGlobs
    )
    Get-ChildItem -Path $Root -Recurse -File -Force -ErrorAction SilentlyContinue |
        Where-Object {
            $rel = $_.FullName.Substring($Root.Length).TrimStart('\', '/').Replace('\', '/')
            $leaf = Split-Path $rel -Leaf
            # 目录排除: 整目录子树跳过, 用 StartsWith + Contains 处理子目录
            $dirHit  = $ExclDirs  | Where-Object { $rel.StartsWith("$_/") -or $rel.Contains("/$_/") -or $rel -eq $_ }
            $defDir  = $DefDirs   | Where-Object { $rel.StartsWith("$_/") -or $rel.Contains("/$_/") -or $rel -eq $_ }
            $fileHit = $ExclFiles | Where-Object { $rel -eq $_ -or $leaf -eq $_ }
            $globHit = $ExclGlobs | Where-Object { $rel -like $_ }
            # default 排除: 同时支持纯文件名 + 路径前缀 (子目录)
            $defHit  = $DefExcl   | Where-Object { $rel -eq $_ -or $leaf -eq $_ -or $rel.StartsWith("$_/") -or $rel.Contains("/$_/") }
            $giFile  = $GitignoreFiles | Where-Object { $leaf -like $_ }
            $giGlob  = $GitignoreGlobs | Where-Object { $rel -like $_ }
            -not $dirHit -and -not $defDir -and -not $fileHit -and -not $globHit -and -not $defHit -and -not $giFile -and -not $giGlob
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

function Test-Pattern {
    param([string]$RelPath, [string[]]$Patterns)
    $leaf = Split-Path $RelPath -Leaf
    foreach ($p in $Patterns) {
        # 三种匹配: 全路径, 纯文件名, 通配符 (e.g. samples/cloud_log_plugin/case_045*)
        if ($RelPath -eq $p) { return $true }
        if ($leaf -eq $p)   { return $true }
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

# ── 读 .gitignore 模式 (用于额外过滤, 避免把 publish2 端忽略的中间产物当 raw added) ──
# 返回 hashtable: { Dirs = [string[]], Files = [string[]], Globs = [string[]] }
#   Dirs  : 目录模式 (e.g. 'target', 'other')         - 整个目录子树跳过
#   Files : 纯文件名/通配 (e.g. '.DS_Store', '*.bak') - 只匹配文件名
#   Globs : 路径通配 (e.g. 'docs/audit/*')            - 整路径 -like 匹配
function Read-GitignorePatterns {
    param([string]$Root)
    $gitignore = Join-Path $Root ".gitignore"
    $result = @{ Dirs = @(); Files = @(); Globs = @() }
    if (-not (Test-Path $gitignore)) { return $result }
    $lines = Get-Content $gitignore -ErrorAction SilentlyContinue
    foreach ($raw in $lines) {
        $line = $raw.Trim()
        if (-not $line) { continue }
        if ($line.StartsWith('#')) { continue }                    # 注释
        if ($line.StartsWith('!')) { continue }                    # 反模式
        # gitignore 通配符 ** 任意层级 → PowerShell 的 * 即可
        $p = $line.Replace('**', '*')
        # 末尾 / 视为目录: 整目录子树跳过 (e.g. 'target/' → 'target')
        if ($p.EndsWith('/')) {
            $p = $p.TrimEnd('/')
            if ($p) { $result.Dirs += $p }
            continue
        }
        if (-not $p) { continue }
        if ($p -notmatch '[/\\]') {                                # 无分隔符: 纯文件名/纯通配
            $result.Files += $p
        } else {
            $result.Globs += "*$p"
        }
    }
    return $result
}

# ── 扫描 ──────────────────────────────────────────────────────────────
Write-Host "[scan] 计算文件 hash ..." -ForegroundColor Cyan
Write-Host "  src: $Src"
# ── 拆分 defaultExcludePatterns: 末尾 / 的视为目录, 走整目录排除 ──
$defaultExcludeDirs  = @($defaultExcludePatterns | Where-Object { $_.EndsWith('/') } | ForEach-Object { $_.TrimEnd('/') })
$defaultExcludeFiles = @($defaultExcludePatterns | Where-Object { -not $_.EndsWith('/') })
# src 应用 defaultExclude, 用户用 -IncludeXxx 显式打开的项不排除
$srcDefaultExcl = @($defaultExcludeFiles)
if ($IncludeGitignore) { $srcDefaultExcl = $srcDefaultExcl | Where-Object { $_ -ne '.gitignore' } }
if ($IncludeLock) {
    $lockFiles = @('Cargo.lock', 'package-lock.json', 'pnpm-lock.yaml', 'yarn.lock')
    $srcDefaultExcl = $srcDefaultExcl | Where-Object { $lockFiles -notcontains $_ }
}
if ($IncludeManifest) {
    $srcDefaultExcl = $srcDefaultExcl | Where-Object { $_ -ne 'Cargo.toml' -and $_ -ne 'package.json' }
}
# 自动读 src + dst 端 .gitignore 模式, 主仓/发布仓各自忽略的中间产物不进 diff
$srcGi = Read-GitignorePatterns $Src
$dstGi = Read-GitignorePatterns $Dst
# 合并: 目录名 + 文件名 + path-glob (去重 + 不空)
$giDirs  = @($defaultExcludeDirs + $srcGi.Dirs + $dstGi.Dirs) | Where-Object { $_ } | Select-Object -Unique
$giFiles = @($srcGi.Files + $dstGi.Files) | Where-Object { $_ } | Select-Object -Unique
$giGlobs = @($srcGi.Globs + $dstGi.Globs) | Where-Object { $_ } | Select-Object -Unique
Write-Host ("       .gitignore 模式: {0} 目录, {1} 文件名, {2} 路径通配" -f $giDirs.Count, $giFiles.Count, $giGlobs.Count) -ForegroundColor DarkGray
$srcFiles = @(Get-FileHashMap $Src $excludeDirs $excludeFiles $runtimeExcludeGlobs $srcDefaultExcl $giDirs $giFiles $giGlobs)
Write-Host "       $($srcFiles.Count) 个文件" -ForegroundColor Green
Write-Host "  dst: $Dst"
$dstFiles = @(Get-FileHashMap $Dst $excludeDirs $excludeFiles @() @() $giDirs $giFiles $giGlobs)
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
$defaultExcludedHits = @()   # 被 defaultExclude 排除的 (两边各自管)

$added = @()
foreach ($f in $rawAdded) {
    if (Test-Pattern $f.RelPath $protectedPatterns)        { $protectedHits += $f; continue }
    if (Test-Pattern $f.RelPath $userExcludePatterns)      { $userExcludedHits += $f; continue }
    if (Test-Pattern $f.RelPath $defaultExcludePatterns)   { $defaultExcludedHits += $f; continue }
    $added += $f
}
$modified = @()
foreach ($f in $rawModified) {
    if (Test-Pattern $f.RelPath $protectedPatterns)        { $protectedHits += $f; continue }
    if (Test-Pattern $f.RelPath $userExcludePatterns)      { $userExcludedHits += $f; continue }
    if (Test-Pattern $f.RelPath $defaultExcludePatterns)   { $defaultExcludedHits += $f; continue }
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
if ($defaultExcludedHits.Count -gt 0) {
    Write-Host ("  default-excluded  : {0}  (两边各自管, 默认不参与同步; 用 -IncludeXxx 打开)" -f $defaultExcludedHits.Count) -ForegroundColor DarkGray
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
if ($AutoA) {
    $choice = "A"
    Write-Host "  [-AutoA] 非交互: 自动选 A 同步所有 modified" -ForegroundColor DarkGray
} else {
    $choice = (Read-Host "选择 [P/A/D/N/Q]").Trim().ToUpper()
    if (-not $choice) { $choice = "P" }
}

$toSync = @()

switch ($choice) {
    'N' { Write-Host "[skip] 不同步" -ForegroundColor Yellow; exit 0 }
    'Q' { Write-Host "[quit] 退出" -ForegroundColor Yellow; exit 2 }

    'P' {
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
if ($AutoA) {
    $confirm = "y"
    Write-Host "  [-AutoA] 非交互: 跳过确认, 直接同步" -ForegroundColor DarkGray
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

# ── 跑脱敏扫 (publish2 端) ───────────────────────────────────────────
$sweepScript = Join-Path $Dst "scripts\private-sweep-verify.ps1"
if (Test-Path $sweepScript) {
    Write-Host ""
    Write-Host "=== 跑脱敏扫 (publish2 端) ===" -ForegroundColor Cyan
    & pwsh -NoProfile -ExecutionPolicy Bypass -File $sweepScript
} else {
    Write-Host "[warn] 脱敏扫脚本不存在: $sweepScript" -ForegroundColor Yellow
}

# ── 提示下一步 (publish2 端的 git 操作不进 sync 脚本) ─────────────────
if ($ShowNextSteps -and $okCount -gt 0) {
    $boxWidth = 70
    $bar = '=' * $boxWidth

    Write-Host ""
    Write-Host $bar -ForegroundColor Green
    Write-Host ("  [done] 同步完成 ({0} 个文件)。下一步到 publish2 端跑:" -f $okCount) -ForegroundColor Green
    Write-Host $bar -ForegroundColor Green
    Write-Host ""
    Write-Host "    cd C:\git_work\TokenSlim-publish2" -ForegroundColor White
    Write-Host "    node scripts/bump-version.mjs 0.2.6" -ForegroundColor White
    Write-Host "    git add -A" -ForegroundColor White
    Write-Host '    git commit -m "chore(release): v0.2.6"' -ForegroundColor White
    Write-Host "    git tag v0.2.6" -ForegroundColor White
    Write-Host "    git push origin main --tags" -ForegroundColor White
    Write-Host ""
    Write-Host $bar -ForegroundColor Green

    # 写备忘文件到主仓根 (避免忘记), 同步脚本会跳过 .sync-* 文件
    $memoPath = Join-Path $Src ".sync-publish2-next-steps.ps1"
    $memoContent = @"
# TokenSlim -> TokenSlim-publish2 同步后, publish2 端需要执行的命令
# 自动生成于: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')
# 同步来源  : $Src
# 同步目标  : $Dst
# 成功文件数: $okCount
# 失败文件数: $failCount
# 备忘文件  : $memoPath
#
# 用法: 直接复制下面命令, 粘到 publish2 端 PowerShell 跑

cd C:\git_work\TokenSlim-publish2
node scripts/bump-version.mjs 0.2.6
git add -A
git commit -m "chore(release): v0.2.6"
git tag v0.2.6
git push origin main --tags
"@
    try {
        Set-Content -Path $memoPath -Value $memoContent -Encoding utf8
        Write-Host ("  备忘已写入: {0}" -f $memoPath) -ForegroundColor DarkGray
    } catch {
        Write-Host ("  [warn] 备忘写入失败: {0}" -f $_) -ForegroundColor Yellow
    }
    Write-Host ""
}
