#!/usr/bin/env pwsh
# 把 TokenSlim-publish 的 working tree 复制到 TokenSlim-publish2
# 排除: .git, dist, target, node_modules, __pycache__, .gradle, .vs, .idea
$ErrorActionPreference = "Stop"

$src = "C:\git_work\TokenSlim-publish"
$dst = "C:\git_work\TokenSlim-publish2"

if (Test-Path $dst) {
    Write-Host "[ERR] $dst 已存在，请先删除或重命名" -ForegroundColor Red
    exit 1
}

# 排除目录名（不递归 .git 内的内容，节约时间）
$excludeDirs = @('.git', 'dist', 'target', 'node_modules', '__pycache__',
                 '.gradle', '.vs', '.idea', '.pytest_cache', 'out')

# robocopy 镜像模式：/MIR 会删除 dst 中 src 没有的文件
# 我们想要纯复制, 用 /E (复制子目录, 包括空目录)
# /XD 排除目录, /XF 排除文件
$xdArgs = $excludeDirs | ForEach-Object { "/XD"; $_ }

Write-Host "[info] src: $src" -ForegroundColor Cyan
Write-Host "[info] dst: $dst" -ForegroundColor Cyan
Write-Host "[info] exclude: $($excludeDirs -join ', ')" -ForegroundColor Cyan

# robocopy 返回码: 0=无变化 1=复制成功 2=额外文件/目录 3=两者
# 用 > $null 抑制常规日志
$robolog = robocopy $src $dst /E /NFL /NDL /NJH /NJS /NC /NS /NP @xdArgs 2>&1
$rc = $LASTEXITCODE

Write-Host ""
Write-Host "[info] robocopy exit code: $rc (1=成功, 0=无变化, ≥2 有警告)" -ForegroundColor Cyan
Write-Host "[info] dst 大小: $((Get-Item $dst).Length) bytes" -ForegroundColor Cyan
Write-Host "[info] 顶层条目: $((Get-ChildItem $dst -Force | Measure-Object).Count)" -ForegroundColor Cyan

# 验证: 不应有 .git 目录
$hasGit = Test-Path "$dst\.git"
$hasDist = Test-Path "$dst\dist"
$hasTarget = Test-Path "$dst\target"
$hasNodeModules = Test-Path "$dst\node_modules"

Write-Host ""
Write-Host "[verify] .git 存在? $hasGit (期望 False)" -ForegroundColor $(if ($hasGit) { "Red" } else { "Green" })
Write-Host "[verify] dist 存在? $hasDist (期望 False)" -ForegroundColor $(if ($hasDist) { "Red" } else { "Green" })
Write-Host "[verify] target 存在? $hasTarget (期望 False)" -ForegroundColor $(if ($hasTarget) { "Red" } else { "Green" })
Write-Host "[verify] node_modules 存在? $hasNodeModules (期望 False)" -ForegroundColor $(if ($hasNodeModules) { "Red" } else { "Green" })

if ($hasGit -or $hasDist -or $hasTarget -or $hasNodeModules) {
    Write-Host "[FAIL] 复制结果含应排除目录，请检查" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "[OK] 复制完成，可进入 $dst 执行 git init" -ForegroundColor Green
