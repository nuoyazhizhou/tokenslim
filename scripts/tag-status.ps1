#!/usr/bin/env pwsh
# 列出每个 tag 相对脱敏 commit f1f2550 的位置
$ErrorActionPreference = "Continue"
Set-Location C:\git_work\TokenSlim-publish

Write-Host "--- 脱敏锚点 commit ---" -ForegroundColor Cyan
$anchor = git rev-parse f1f2550
Write-Host "  f1f2550 -> $anchor"

Write-Host ""
Write-Host "--- 各 tag 状态 ---" -ForegroundColor Cyan
git tag -l "v*" | ForEach-Object {
    $t = $_
    $sha = git rev-parse "$t" 2>$null
    if (-not $sha) { return }
    # 看 anchor 是不是 sha 的祖先
    $isAnc = git merge-base --is-ancestor f1f2550 $sha 2>$null
    if ($LASTEXITCODE -eq 0) {
        Write-Host ("  {0,-10}  {1}  OK 在 f1f2550 之后 (可推)" -f $t, $sha) -ForegroundColor Green
    } else {
        Write-Host ("  {0,-10}  {1}  !! 在 f1f2550 之前 (含泄漏，禁推)" -f $t, $sha) -ForegroundColor Red
    }
}
