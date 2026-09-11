$source = "C:\git_work\TokenSlim\samples\vcs"
$baseTarget = "C:\git_work\TokenSlim\samples"

# 需要分类的 VCS 工具列表
$vcsList = @("git", "svn", "hg", "p4", "cvs", "bzr", "fossil", "darcs", "gh", "glab", "repo", "az", "gerrit")

foreach ($vcs in $vcsList) {
    $target = Join-Path $baseTarget "vcs_${vcs}_plugin"

    # 如果目标目录不存在，则创建
    if (-not (Test-Path $target)) {
        New-Item -ItemType Directory -Force -Path $target | Out-Null
    }

    # 正则匹配：例如 "git_" 或 "case_123_git_"
    $regex = "^(case_\d+_)?${vcs}_"

    # 获取符合条件的文件
    $filesToMove = Get-ChildItem -Path $source -Filter "*.log" | Where-Object { $_.Name -match $regex }

    if ($filesToMove) {
        $filesToMove | Move-Item -Destination $target -Force
        Write-Host "成功移动 $($filesToMove.Count) 个文件到 vcs_${vcs}_plugin" -ForegroundColor Green
    } else {
        Write-Host "未找到属于 $vcs 的日志文件" -ForegroundColor DarkGray
    }
} # <--- 就是你刚才漏掉了这个大括号！

Write-Host "🎉 所有工具的 Case 日志迁移彻底完成！" -ForegroundColor Cyan